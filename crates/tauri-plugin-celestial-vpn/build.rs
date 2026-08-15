const COMMANDS: &[&str] = &["startVpn", "stopVpn"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").build();

    build_mihomo_wrapper();
    build_xray_wrapper();
}

/// The ABIs the xray relay ships for.
///
/// Not all of them, and the reason is weight rather than capability: the core builds cleanly
/// for every Android ABI, but it adds ~31 MB per ABI to a universal APK that is already
/// 351 MB. arm64-v8a is every Android phone made in the last several years.
///
/// Leaving an ABI out costs those devices the relay, not the app: the relay reports itself
/// unsupported and the client dials nodes natively, which is the same path mobile took before
/// any of this existed. Adding one back is a line here.
const XRAY_ABIS: &[&str] = &["arm64-v8a"];

/// Cross-compiles the mihomo cgo wrapper (golang/wrapper) for the Android
/// target currently being built and drops the resulting .so directly into
/// this Gradle module's `src/main/jniLibs/<abi>/` — the standard convention
/// for shipping prebuilt native libraries from an Android library module,
/// so AGP bundles it into the final APK with no extra Tauri-side wiring.
fn build_mihomo_wrapper() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let Some((go_arch, ndk_triple, android_abi)) = android_target_info(&target) else {
        return; // desktop host build (e.g. `cargo check` on Windows) — nothing to do
    };

    println!("cargo:rerun-if-changed=golang/wrapper/main.go");
    println!("cargo:rerun-if-changed=golang/wrapper/go.mod");

    let ndk_home = std::env::var("NDK_HOME")
        .or_else(|_| std::env::var("ANDROID_NDK_HOME"))
        .expect("NDK_HOME or ANDROID_NDK_HOME must be set to build the mihomo wrapper for Android");

    let host_tag = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    let ext = if cfg!(target_os = "windows") { ".cmd" } else { "" };
    let cc = format!("{ndk_home}/toolchains/llvm/prebuilt/{host_tag}/bin/{ndk_triple}{ext}");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let wrapper_dir = std::path::Path::new(&manifest_dir).join("golang/wrapper");

    apply_mihomo_patches(std::path::Path::new(&manifest_dir));

    let jni_libs_dir = std::path::Path::new(&manifest_dir)
        .join("android/src/main/jniLibs")
        .join(android_abi);
    std::fs::create_dir_all(&jni_libs_dir).expect("failed to create jniLibs output dir");
    let so_path = jni_libs_dir.join("libmihomo_wrapper.so");

    // constant.Version defaults to a hardcoded "1.10.0" placeholder in
    // golang/mihomo/constant/version.go — mihomo's own Makefile always
    // overrides it via ldflags from `git describe --tags` on the actual
    // checked-out commit. Do the same so `/version` reports the real
    // submodule version instead of that stale placeholder.
    let mihomo_dir = std::path::Path::new(&manifest_dir).join("golang/mihomo");
    let version = std::process::Command::new("git")
        .current_dir(&mihomo_dir)
        .args(["describe", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    // `-s -w` drop the symbol table and DWARF. The Go core is shipped, never debugged
    // on-device, and unstripped it was 77 MB in the APK against ~50 MB stripped — which
    // matters because Play measures the per-device download against a hard ceiling and
    // the arm64 slice alone was 190 MB.
    let ldflags = format!("-s -w -X github.com/metacubex/mihomo/constant.Version={version}");

    let status = std::process::Command::new("go")
        .current_dir(&wrapper_dir)
        .env("CGO_ENABLED", "1")
        .env("GOOS", "android")
        .env("GOARCH", go_arch)
        .env("CC", &cc)
        // mihomo's stock android build tries to read /data/system/packages.xml
        // directly (for per-app TUN routing rules) — a regular sandboxed app
        // can't access that file, so TUN setup fails with a permission error.
        // The `cmfa` build tag (originally for MetaCubeX/ClashMetaForAndroid)
        // swaps in a no-op implementation instead — see
        // golang/mihomo/listener/sing_tun/server_{android,notandroid}.go.
        // `with_gvisor` enables the userspace netstack TUN needs on Android
        // (no raw kernel TUN driver access without gVisor) — same tags
        // ClashMetaForAndroid's own Gradle build uses.
        .args([
            "build",
            "-tags",
            "cmfa,with_gvisor",
            "-buildmode=c-shared",
            "-ldflags",
            &ldflags,
            "-o",
        ])
        .arg(&so_path)
        .arg(".")
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke `go build` (is Go installed and on PATH?): {e}"));

    assert!(
        status.success(),
        "go build failed for the mihomo wrapper (target={target}, GOARCH={go_arch}, CC={cc})"
    );

    // go build -buildmode=c-shared also emits a matching .h header we don't need.
    let _ = std::fs::remove_file(jni_libs_dir.join("libmihomo_wrapper.h"));

    // Tell the Rust linker where to find it too (ffi.rs links against it via
    // #[link(name = "mihomo_wrapper")]) — at runtime the Android dynamic
    // linker resolves it from this same jniLibs/<abi>/ dir once packaged.
    println!("cargo:rustc-link-search=native={}", jni_libs_dir.display());
}

/// Cross-compiles the xray cgo wrapper for the Android target being built.
///
/// Mirrors [`build_mihomo_wrapper`] but has no patches to apply: xray only opens loopback
/// inbounds and dials out, so none of the Android TUN and netlink problems that the mihomo
/// side has to work around apply to it.
fn build_xray_wrapper() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let Some((go_arch, ndk_triple, android_abi)) = android_target_info(&target) else {
        return; // desktop host build — the desktop core is a downloaded sidecar instead
    };
    if !XRAY_ABIS.contains(&android_abi) {
        return;
    }

    println!("cargo:rerun-if-changed=golang/xray-wrapper/main.go");
    println!("cargo:rerun-if-changed=golang/xray-wrapper/go.mod");

    let cc = ndk_clang(ndk_triple);
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let wrapper_dir = std::path::Path::new(&manifest_dir).join("golang/xray-wrapper");

    let jni_libs_dir = std::path::Path::new(&manifest_dir)
        .join("android/src/main/jniLibs")
        .join(android_abi);
    std::fs::create_dir_all(&jni_libs_dir).expect("failed to create jniLibs output dir");
    let so_path = jni_libs_dir.join("libxray_wrapper.so");

    // Same reasoning as the mihomo wrapper: the core is shipped, never debugged on-device,
    // and the symbols are most of the file.
    let status = std::process::Command::new("go")
        .current_dir(&wrapper_dir)
        .env("CGO_ENABLED", "1")
        .env("GOOS", "android")
        .env("GOARCH", go_arch)
        .env("CC", &cc)
        .args(["build", "-buildmode=c-shared", "-ldflags", "-s -w", "-o"])
        .arg(&so_path)
        .arg(".")
        .status()
        .unwrap_or_else(|e| panic!("failed to invoke `go build` (is Go installed and on PATH?): {e}"));

    assert!(
        status.success(),
        "go build failed for the xray wrapper (target={target}, GOARCH={go_arch}, CC={cc})"
    );

    let _ = std::fs::remove_file(jni_libs_dir.join("libxray_wrapper.h"));
    println!("cargo:rustc-link-search=native={}", jni_libs_dir.display());
}

/// The NDK clang wrapper for an Android triple, which is what cgo needs as `CC`.
fn ndk_clang(ndk_triple: &str) -> String {
    let ndk_home = std::env::var("NDK_HOME")
        .or_else(|_| std::env::var("ANDROID_NDK_HOME"))
        .expect("NDK_HOME or ANDROID_NDK_HOME must be set to build the Android core wrappers");

    let host_tag = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    let ext = if cfg!(target_os = "windows") { ".cmd" } else { "" };
    format!("{ndk_home}/toolchains/llvm/prebuilt/{host_tag}/bin/{ndk_triple}{ext}")
}

/// `golang/mihomo` is a pristine submodule of MetaCubeX/mihomo, so our
/// Android-specific core fixes can't just be committed there. They live as
/// patch files in `patches/` and get applied to the checkout right before the
/// Go build.
///
/// Idempotent: a patch that already applies in reverse is treated as applied
/// and skipped, so repeat local builds (where the working tree keeps the
/// change) behave the same as a fresh CI checkout.
fn apply_mihomo_patches(manifest_dir: &std::path::Path) {
    let patches_dir = manifest_dir.join("patches");
    let mihomo_dir = manifest_dir.join("golang/mihomo");

    let Ok(entries) = std::fs::read_dir(&patches_dir) else {
        return;
    };

    let mut patches: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "patch"))
        .collect();
    // Apply in file-name order so numbered patches stack predictably.
    patches.sort();

    for patch in patches {
        println!("cargo:rerun-if-changed={}", patch.display());

        let git_apply = |extra: &[&str]| {
            std::process::Command::new("git")
                .current_dir(&mihomo_dir)
                .arg("apply")
                .args(extra)
                .arg(&patch)
                .status()
        };

        let already_applied = git_apply(&["--reverse", "--check"]).is_ok_and(|s| s.success());
        if already_applied {
            continue;
        }

        let status =
            git_apply(&[]).unwrap_or_else(|e| panic!("failed to invoke `git apply` for {}: {e}", patch.display()));
        assert!(
            status.success(),
            "failed to apply mihomo patch {} — the submodule may have moved past it",
            patch.display()
        );
    }
}

/// Maps a Rust Android target triple to (Go GOARCH, NDK clang wrapper name, Android ABI dir name).
fn android_target_info(target: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match target {
        "aarch64-linux-android" => Some(("arm64", "aarch64-linux-android24-clang", "arm64-v8a")),
        "armv7-linux-androideabi" => Some(("arm", "armv7a-linux-androideabi24-clang", "armeabi-v7a")),
        "i686-linux-android" => Some(("386", "i686-linux-android24-clang", "x86")),
        "x86_64-linux-android" => Some(("amd64", "x86_64-linux-android24-clang", "x86_64")),
        _ => None,
    }
}
