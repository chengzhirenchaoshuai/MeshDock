// 发布版不弹出控制台窗口，请勿删除！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    meshdock_lib::run()
}
