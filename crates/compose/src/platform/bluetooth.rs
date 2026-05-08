//! 蓝牙能力接入骨架（能力接缝思路，置于 platform 一级）。
//!
//! 统一 Rust API 见 [`Bluetooth`]；各平台经 cfg 提供实现。当前返回占位实现
//! （[`StubBluetooth`]），真实后端（CoreBluetooth / Android BluetoothManager / Web Bluetooth）
//! 按平台接入后替换。业务侧统一用 [`Bluetooth::available`] 判能力后调用。

use std::sync::Arc;

/// 已配对/发现的蓝牙设备（最小信息）。
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub id: String,
    pub name: String,
}

/// 蓝牙能力 trait — 业务侧持有 `Box<dyn Bluetooth>` 统一调用。
pub trait Bluetooth: Send + Sync {
    /// 适配器是否可用（含权限）。占位实现返回 false。
    fn available(&self) -> bool {
        false
    }
    /// 开始扫描，发现设备经 `on_found` 回调。占位实现为空操作。
    fn start_scan(&self, _on_found: Arc<dyn Fn(BluetoothDevice) + Send + Sync>) {}
    /// 停止扫描。
    fn stop_scan(&self) {}
    /// 已配对设备列表（占位实现返回空）。
    fn paired(&self) -> Vec<BluetoothDevice> {
        Vec::new()
    }
}

/// 占位实现：未接入真实后端时返回，避免能力缺失导致编译/运行失败。
pub struct StubBluetooth;
impl Bluetooth for StubBluetooth {}

/// 构造当前平台蓝牙实现（当前全部走占位，TODO 各平台接入）。
pub fn bluetooth() -> Box<dyn Bluetooth> {
    Box::new(StubBluetooth)
}
