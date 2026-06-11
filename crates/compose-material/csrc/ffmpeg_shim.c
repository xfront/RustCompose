/* Minimal FFmpeg bridge for compose_material::video.
 * Exposes an opaque decoder context so Rust never touches libav struct layouts.
 * Adapted from /work/study/cui/rui_packages/csrc/ffmpeg_shim.c. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavformat/avio.h>
#include <libavutil/display.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libavutil/time.h>
#include <libswscale/swscale.h>

typedef struct ComposeFfmpegCtx {
    AVFormatContext *fmt;
    AVCodecContext *codec;
    AVStream *stream;
    int stream_index;
    AVPacket *pkt;
    AVFrame *frame;
    struct SwsContext *sws;
    int sws_w, sws_h;
    enum AVPixelFormat sws_fmt;

    const uint8_t *bytes;
    size_t bytes_len, bytes_pos;
    AVIOContext *io;
    unsigned char *io_buffer;

    int eof, input_eof, flushed;
    /* FFmpeg 在 avformat_open_input 失败时会回收 fmt->pb 及其 buffer；成功时
     * AVFMT_FLAG_CUSTOM_IO 下 pb 仍归我们所有，关闭 fmt 后自行释放。 */
    int io_owned;
    char error[256];
} ComposeFfmpegCtx;

static void set_error(ComposeFfmpegCtx *c, const char *what, int err) {
    char buf[AV_ERROR_MAX_STRING_SIZE] = {0};
    if (err != 0) av_strerror(err, buf, sizeof(buf));
    snprintf(c->error, sizeof(c->error), "%s%s%s", what,
             (err != 0 && buf[0]) ? ": " : "", buf);
}

static int memory_read(void *opaque, uint8_t *buf, int buf_size) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)opaque;
    if (!c || c->bytes_pos >= c->bytes_len) return AVERROR_EOF;
    size_t n = c->bytes_len - c->bytes_pos;
    if (n > (size_t)buf_size) n = (size_t)buf_size;
    memcpy(buf, c->bytes + c->bytes_pos, n);
    c->bytes_pos += n;
    return (int)n;
}

static int64_t memory_seek(void *opaque, int64_t offset, int whence) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)opaque;
    if (!c) return AVERROR(EINVAL);
    if (whence == AVSEEK_SIZE) return (int64_t)c->bytes_len;
    int64_t base = 0;
    if (whence == SEEK_CUR) base = (int64_t)c->bytes_pos;
    else if (whence == SEEK_END) base = (int64_t)c->bytes_len;
    int64_t pos = base + offset;
    if (pos < 0) return AVERROR(EINVAL);
    c->bytes_pos = (size_t)pos;
    return pos;
}

void *compose_ffmpeg_open(const char *uri, const uint8_t *bytes, int bytes_len,
                          const char *format_hint, int *width, int *height,
                          int64_t *duration_us, double *frame_rate, int *rotation,
                          int *has_audio, char *codec_name, int codec_name_len,
                          char *err, int err_len) {
    if (err && err_len > 0) err[0] = '\0';
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)calloc(1, sizeof(ComposeFfmpegCtx));
    if (!c) { if (err && err_len) snprintf(err, err_len, "out of memory"); return NULL; }
    if (codec_name && codec_name_len > 0) codec_name[0] = '\0';

    c->fmt = avformat_alloc_context();
    if (!c->fmt) { set_error(c, "avformat_alloc_context failed", 0); goto fail; }
    if (bytes && bytes_len > 0) {
        c->bytes = bytes; c->bytes_len = (size_t)bytes_len; c->bytes_pos = 0;
        c->io_buffer = (unsigned char *)av_malloc(4096);
        c->io = avio_alloc_context(c->io_buffer, 4096, 0, c, memory_read, NULL, memory_seek);
        if (!c->io) { set_error(c, "avio_alloc_context failed", 0); goto fail; }
        c->fmt->pb = c->io;
        c->fmt->flags |= AVFMT_FLAG_CUSTOM_IO;
        c->io_owned = 0; /* 暂由 FFmpeg 托管，open 成功后再收回所有权 */
    }

    AVInputFormat *iformat = NULL;
    if (format_hint && *format_hint) iformat = (AVInputFormat *)av_find_input_format(format_hint);
    AVDictionary *opts = NULL;
    av_dict_set(&opts, "probesize", "5000000", 0);
    av_dict_set(&opts, "analyzeduration", "5000000", 0);
    int ret = avformat_open_input(&c->fmt, (bytes && bytes_len > 0) ? "" : (uri ? uri : ""),
                                  iformat, &opts);
    av_dict_free(&opts);
    if (ret < 0) { c->io = NULL; c->io_buffer = NULL; set_error(c, "open failed", ret); goto fail; }
    c->io_owned = 1;
    ret = avformat_find_stream_info(c->fmt, NULL);
    if (ret < 0) { set_error(c, "find_stream_info failed", ret); goto fail; }

    const AVCodec *dec = NULL;
    int idx = av_find_best_stream(c->fmt, AVMEDIA_TYPE_VIDEO, -1, -1, &dec, 0);
    if (idx < 0 || !dec) { set_error(c, "no video stream", idx); goto fail; }
    c->stream_index = idx;
    c->stream = c->fmt->streams[idx];

    c->codec = avcodec_alloc_context3(dec);
    if (!c->codec) { set_error(c, "avcodec_alloc_context3 failed", 0); goto fail; }
    ret = avcodec_parameters_to_context(c->codec, c->stream->codecpar);
    if (ret < 0) { set_error(c, "parameters_to_context failed", ret); goto fail; }
    ret = avcodec_open2(c->codec, dec, NULL);
    if (ret < 0) { set_error(c, "avcodec_open2 failed", ret); goto fail; }

    c->pkt = av_packet_alloc();
    c->frame = av_frame_alloc();
    if (!c->pkt || !c->frame) { set_error(c, "packet/frame alloc failed", 0); goto fail; }

    if (width) *width = c->codec->width > 0 ? c->codec->width : c->stream->codecpar->width;
    if (height) *height = c->codec->height > 0 ? c->codec->height : c->stream->codecpar->height;
    if (duration_us) {
        int64_t d = c->fmt->duration;
        if (d <= 0 && c->stream->duration > 0) {
            d = av_rescale_q(c->stream->duration, c->stream->time_base, AV_TIME_BASE_Q);
        }
        *duration_us = d > 0 ? d : 0;
    }
    if (frame_rate) {
        AVRational r = c->stream->avg_frame_rate;
        if (r.num <= 0 || r.den <= 0) r = c->stream->r_frame_rate;
        *frame_rate = (r.num > 0 && r.den > 0) ? av_q2d(r) : 0.0;
    }
    if (rotation) {
        int rot = 0;
        const AVPacketSideData *sd = av_packet_side_data_get(c->stream->codecpar->coded_side_data,
                                                             c->stream->codecpar->nb_coded_side_data,
                                                             AV_PKT_DATA_DISPLAYMATRIX);
        if (sd) {
            double theta = -av_display_rotation_get((const int32_t *)sd->data);
            rot = (int)llround(theta / 90.0) * 90;
            rot %= 360; if (rot < 0) rot += 360;
        }
        *rotation = rot;
    }
    if (has_audio) {
        *has_audio = 0;
        for (unsigned i = 0; i < c->fmt->nb_streams; ++i) {
            if (c->fmt->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) { *has_audio = 1; break; }
        }
    }
    if (codec_name && codec_name_len > 0)
        snprintf(codec_name, codec_name_len, "%s", dec->name ? dec->name : "");
    return c;

fail:
    if (c) {
        if (c->fmt && c->io) c->fmt->pb = NULL;
        if (c->fmt) avformat_close_input(&c->fmt);
        if (c->io) av_freep(&c->io);
        if (c->io_buffer) av_freep(&c->io_buffer);
        avcodec_free_context(&c->codec);
        av_packet_free(&c->pkt);
        av_frame_free(&c->frame);
        if (err && err_len > 0) snprintf(err, err_len, "%s", c->error[0] ? c->error : "open failed");
        free(c);
    }
    return NULL;
}

static int convert_frame(ComposeFfmpegCtx *c, uint8_t **out, int *w, int *h, int64_t *pts_us) {
    int width = c->frame->width > 0 ? c->frame->width : c->codec->width;
    int height = c->frame->height > 0 ? c->frame->height : c->codec->height;
    if (width <= 0 || height <= 0) { set_error(c, "invalid frame size", 0); return -1; }
    if (!c->sws || c->sws_w != width || c->sws_h != height || c->sws_fmt != c->frame->format) {
        if (c->sws) sws_freeContext(c->sws);
        c->sws = sws_getContext(width, height, (enum AVPixelFormat)c->frame->format,
                                width, height, AV_PIX_FMT_RGBA, SWS_BILINEAR,
                                NULL, NULL, NULL);
        c->sws_w = width; c->sws_h = height; c->sws_fmt = (enum AVPixelFormat)c->frame->format;
        if (!c->sws) { set_error(c, "sws_getContext failed", 0); return -1; }
    }
    uint8_t *rgba = (uint8_t *)malloc((size_t)width * height * 4);
    if (!rgba) { set_error(c, "rgba alloc failed", 0); return -1; }
    uint8_t *dst[1] = { rgba };
    int dst_ls[1] = { width * 4 };
    int n = sws_scale(c->sws, (const uint8_t * const *)c->frame->data,
                      c->frame->linesize, 0, height, dst, dst_ls);
    if (n <= 0) { free(rgba); set_error(c, "sws_scale failed", n); return -1; }
    int64_t pts = c->frame->best_effort_timestamp;
    if (pts == AV_NOPTS_VALUE) pts = c->frame->pts;
    if (pts == AV_NOPTS_VALUE) pts = 0;
    *pts_us = av_rescale_q(pts, c->stream->time_base, AV_TIME_BASE_Q);
    *out = rgba; *w = width; *h = height;
    return 1;
}

int compose_ffmpeg_read(void *ctx_ptr, uint8_t **rgba_out, int *width, int *height, int64_t *pts_us) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)ctx_ptr;
    if (!c || !rgba_out) return -1;
    *rgba_out = NULL;
    for (;;) {
        int ret = avcodec_receive_frame(c->codec, c->frame);
        if (ret == 0) return convert_frame(c, rgba_out, width, height, pts_us);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            /* need more input, or drain finished */
        } else if (ret < 0) {
            set_error(c, "receive_frame failed", ret);
            return -1;
        }
        if (ret == AVERROR_EOF) { c->eof = 1; return 0; }

        if (c->input_eof) {
            if (!c->flushed) {
                ret = avcodec_send_packet(c->codec, NULL);
                if (ret < 0 && ret != AVERROR(EAGAIN)) { set_error(c, "flush send failed", ret); return -1; }
                c->flushed = 1;
            } else { c->eof = 1; return 0; }
            continue;
        }

        ret = av_read_frame(c->fmt, c->pkt);
        if (ret >= 0) {
            if (c->pkt->stream_index != c->stream_index) { av_packet_unref(c->pkt); continue; }
            ret = avcodec_send_packet(c->codec, c->pkt);
            av_packet_unref(c->pkt);
            if (ret < 0 && ret != AVERROR(EAGAIN)) { set_error(c, "send_packet failed", ret); return -1; }
            continue;
        }
        if (ret == AVERROR(EAGAIN)) continue;
        if (ret == AVERROR_EOF) { c->input_eof = 1; continue; }
        set_error(c, "read_frame failed", ret);
        return -1;
    }
}

void compose_ffmpeg_release_buffer(void *ptr) { free(ptr); }

void compose_ffmpeg_seek(void *ctx_ptr, int64_t target_us) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)ctx_ptr;
    if (!c || !c->fmt || !c->stream) return;
    int64_t ts = av_rescale_q(target_us, AV_TIME_BASE_Q, c->stream->time_base);
    int ret = av_seek_frame(c->fmt, c->stream_index, ts, AVSEEK_FLAG_BACKWARD);
    if (ret < 0) { set_error(c, "seek failed", ret); return; }
    avcodec_flush_buffers(c->codec);
    c->eof = 0; c->input_eof = 0; c->flushed = 0;
    c->error[0] = '\0';
}

int compose_ffmpeg_at_end(void *ctx_ptr) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)ctx_ptr;
    return c ? c->eof : 1;
}

const char *compose_ffmpeg_error(void *ctx_ptr) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)ctx_ptr;
    return (c && c->error[0]) ? c->error : "";
}

void compose_ffmpeg_close(void *ctx_ptr) {
    ComposeFfmpegCtx *c = (ComposeFfmpegCtx *)ctx_ptr;
    if (!c) return;
    if (c->fmt && c->io_owned && c->io) c->fmt->pb = NULL;
    if (c->fmt) avformat_close_input(&c->fmt);
    if (c->io_owned && c->io) av_freep(&c->io);
    if (c->io_owned && c->io_buffer) av_freep(&c->io_buffer);
    if (c->sws) sws_freeContext(c->sws);
    avcodec_free_context(&c->codec);
    av_packet_free(&c->pkt);
    av_frame_free(&c->frame);
    free(c);
}
