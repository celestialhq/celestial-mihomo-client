const COMMANDS: &[&str] = &["startVpn", "stopVpn"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
