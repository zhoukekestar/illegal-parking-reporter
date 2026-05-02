// 阻止 Windows release 构建打开 console 窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    illegal_parking_reporter_lib::run()
}
