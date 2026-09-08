//! Link the optional C libraries eSpeak NG was built against.
//!
//! `espeak-rs-sys` builds the vendored espeak-ng with CMake, whose configure
//! step auto-detects `libpcaudio` (audio output) and `libsonic` (fast speech
//! rates) and compiles against them when the host has them - but it never
//! emits link directives for either. On a machine that has them installed
//! (Arch with `pcaudiolib`/`libsonic`, for instance) any binary that links
//! this crate then fails with undefined `audio_object_*` / `sonic*` symbols.
//!
//! Dependency build scripts run before ours and their output directories are
//! siblings of `OUT_DIR`, so we can read espeak-ng's CMake cache and link
//! exactly what it was configured with. Where a library was not found, or was
//! fetched and compiled into `libespeak-ng.a`, the cache says so and we emit
//! nothing.
//!
//! These are `rustc-link-lib`/`rustc-link-search` rather than raw link args on
//! purpose: only those propagate to the binaries that depend on this crate,
//! and rustc orders them right after our own rlib - after the espeak-ng
//! objects that reference them, which is what `--as-needed` requires.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    link_espeak_optional_libs(Path::new(&out_dir));
}

fn link_espeak_optional_libs(out_dir: &Path) {
    // OUT_DIR = <profile>/build/kokoro-micro-<hash>/out  ->  <profile>/build
    let Some(build_dir) = out_dir.parent().and_then(Path::parent) else {
        return;
    };
    let Ok(entries) = fs::read_dir(build_dir) else {
        return;
    };

    let mut caches: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("espeak-rs-sys-"))
        .map(|e| e.path().join("out").join("build").join("CMakeCache.txt"))
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let mtime = fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((mtime, p))
        })
        .collect();
    // Prefer the most recent configuration if stale ones linger.
    caches.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    let Some((_, cache_path)) = caches.into_iter().next() else {
        return;
    };
    // espeak-rs-sys sometimes installs a copy of espeak-ng-data next to the
    // library it built. When it does, remember where: it is the best guess for
    // a binary run straight out of `target/`, and far better than the data
    // path espeak-ng compiles into itself, which points into a build directory
    // that may not survive a `cargo clean` or a rebuild under a new hash.
    if let Some(sys_out) = cache_path.parent().and_then(Path::parent) {
        let share = sys_out.join("share");
        if share.join("espeak-ng-data").join("phontab").is_file() {
            println!("cargo:rustc-env=KOKORO_ESPEAK_BUILD_DATA_DIR={}", share.display());
        }
    }

    let Ok(cache) = fs::read_to_string(&cache_path) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", cache_path.display());

    let value = |key: &str| -> Option<String> {
        cache.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            let (name, _ty) = k.split_once(':')?;
            (name == key).then(|| v.trim().to_string())
        })
    };

    for (use_flag, lib_key) in [
        ("USE_LIBPCAUDIO", "PCAUDIO_LIB"),
        ("USE_LIBSONIC", "SONIC_LIB"),
    ] {
        if !value(use_flag).is_some_and(|v| v.eq_ignore_ascii_case("ON")) {
            continue;
        }
        let Some(lib) = value(lib_key) else { continue };
        if lib.ends_with("-NOTFOUND") {
            continue;
        }
        let lib_path = Path::new(&lib);
        if !lib_path.is_file() {
            continue;
        }
        let Some(dir) = lib_path.parent() else { continue };
        let Some(stem) = lib_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // libfoo.so / libfoo.so.0 / libfoo.a -> foo. Linking by name rather
        // than by path keeps a build-host absolute path (libsonic carries no
        // SONAME) out of the resulting binary's NEEDED entries.
        let name = stem.strip_prefix("lib").unwrap_or(stem);
        let name = name.split(".so").next().unwrap_or(name);
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=dylib={name}");
    }
}
