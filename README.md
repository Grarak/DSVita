# DSVita

[![Rust](https://github.com/Grarak/DSVita/actions/workflows/rust.yml/badge.svg)](https://github.com/Grarak/DSVita/actions/workflows/rust.yml)

Fast NDS Emulator for ARM32/PSVita

## Status

[![DSVita Mario Kart DS](http://img.youtube.com/vi/T5SaVkuuhbM/0.jpg)](https://www.youtube.com/watch?v=T5SaVkuuhbM "DSVita Mario Kart DS")

Most games run, with these caveats:

- 3D rendering is mostly implemented
    - Z fighting can occur
    - Games using 3D on both screens will have bad framerates and stutters
    - Some effects like fog are unimplemented
- 2D rendering is mostly complete
- ARM7 HLE (the `Arm7 Emulation` setting, `Hle`) doesn't work with some games
    - If a game doesn't boot, gets stuck or crashes, switch to `SoundHle` or `AccurateLle` —
      each step is more compatible but slower
- Auto frameskip is always used
    - The FPS counter will often show 20-30 fps even when the game logic runs at full speed
- No scanline rendering, so games that update VRAM mid-frame will not render correctly
    - Few games do this; those that do mostly use it for scrolling text

## Installation/Setup

- Grab the latest vpk from [releases](https://github.com/Grarak/DSVita/releases)
- Install `libshacccg.suprx`, follow
  this [guide](https://cimmerian.gitbook.io/vita-troubleshooting-guide/shader-compiler/extract-libshacccg.suprx) or just install and open VitaDB
- Install `kubridge.skprx` version >= 0.3.1 from https://github.com/bythos14/kubridge/releases
  - Make sure this plugin is in the `*KERNEL` section, otherwise the app might crash upon opening
  - If you have the wrong version installed, the app will either crash or will not be able to launch any games
- It's strongly recommended to overclock your Vita to 500MHz
- Create the folder `ux0:data/dsvita` and put your roms there
    - They must have the `.nds` file extension
- Check out the [compatibility list](https://github.com/Grarak/DSVita/wiki/Compatibility-list) for popular games

## Bug reporting

Feel free to create an issue if you run into problems. Before reporting, please make sure the
issue still occurs with the `Arm7 Emulation` setting set to `AccurateLle`.

## Building

Install [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

You need to have both llvm-18 and llvm-21 installed.
- llvm-18 is required for bindgen, newer versions struggle to parse class sizes correctly
- llvm-21 is used for building C/C++ libraries

Clone the repo
```bash
$ git clone --recurse-submodules https://github.com/Grarak/DSVita.git
```

### Linux
Get an armhf sysroot with libsdl2 development packages installed
- On ubuntu >= 22.04
    ```bash
    $ sudo apt install debootstrap qemu-user-static
    $ sudo debootstrap --foreign --variant=buildd --arch=armhf jammy ./ubuntu-armhf https://ports.ubuntu.com
    $ sudo cp /usr/bin/qemu-arm-static ubuntu-armhf/usr/bin/
    $ sudo chroot ubuntu-armhf /debootstrap/debootstrap --second-stage
    $ apt update && apt install libsdl2-dev # If apt can't find the package make sure the source has "main restricted universe multiverse" defined in /etc/apt
    ```
```bash
$ LIBCLANG_PATH=<path to llvm-18 library> DSVITA_SYSROOT=<path to armhf sysroot> cargo build --target thumbv7neon-unknown-linux-gnueabihf --release
```

### Development environment file
The build variables above and the helper scripts in `tools/` (launching under qemu, remote
test box runs, screenshots, instruction traces — see `tools/README.md`) read their paths from
environment variables. Copy the example file and fill in your own setup:
```bash
$ cp .env.example .env   # then edit .env with your paths
```
The `tools/` scripts source `.env` automatically. To use it for cargo builds:
```bash
$ set -a; . ./.env; set +a
$ cargo build --target thumbv7neon-unknown-linux-gnueabihf --release
```
`.env` is gitignored — it's your machine-specific setup. Development background (JIT/interpreter
invariants, debugging playbook) lives in `DEVELOPMENT.md`.

### Vita
- Install [Vitasdk](https://vitasdk.org/)
- Install [cargo vita](https://github.com/vita-rust/cargo-vita)
```bash
$ LIBCLANG_PATH=<path to llvm-18 library> cargo vita build vpk -- --release
```

### Android (experimental)
The Android port is still experimental and has no releases yet. It needs an Android NDK
(sysroot only, the compiling is done by host clang-21):
```bash
$ export ANDROID_NDK_HOME=<path to ndk>
$ cd android && ./gradlew assembleDebug   # builds libdsvita.so via cargo and packs the apk
```

### Final optimized release build
To obtain the most optimized build you need to use a [patched rust compiler](https://github.com/Grarak/rust) due to LTO incompatibility.
The upstream rust compiler doesn't set the target cpu and target features in the callsite attributes
which prevents the clang linker from inlining cross language functions.
```bash
$ RUSTC=<path to compiled rustc> LIBCLANG_PATH=<path to llvm-18 library> RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none -Zub-checks=no -Zsaturating-float-casts=no -Ztrap-unreachable=no -Zmir-opt-level=4 -Clinker-plugin-lto -Clto=fat -Zunstable-options -Cpanic=immediate-abort" cargo vita build vpk -- --release
```

The toolchain is also pinned to an older rust nightly (see `rust-toolchain.toml`), the last one built against llvm-21 — newer llvm versions cause performance regressions.

### Stream top screen over udcd-uvc (experimental)
- Install (replace your existing udcd-uvc install) https://github.com/Grarak/vita-udcd-uvc/releases/tag/1.0 under the `*KERNEL` section of your config.txt
- Install https://github.com/GrapheneCt/CapUnlocker
- Enable the stream top screen setting
- If you only get a black screen, reboot your vita (you might need multiple reboots)

## Credits

- [NooDS](https://github.com/Hydr8gon/NooDS) was used as reference. A lot of code was taken from there.
- [melonDS](https://github.com/melonDS-emu/melonDS) for ARM7 HLE implementation and jit optimizations.
- [DesmumePSPExperimental](https://github.com/Xiro28/DesmumePSPExperimental) for ARM7 HLE implementation.
- [pokediamond](https://github.com/pret/pokediamond) for ARM7 HLE implementation.
- [DSHBA](https://github.com/DenSinH/DSHBA) Copied some PPU hardware acceleration implementation (Thanks for xiro28
  linking me the repo)
- [vitaGL](https://github.com/Rinnegatamante/vitaGL) 2D/3D hardware acceleration wouldn't be possible without it
- [Tonc](https://www.coranac.com/tonc/text/toc.htm) GBA PPU documentation
- [GBATEK](http://problemkaputt.de/gbatek-index.htm) GBA/NDS documentation
- [kubridge](https://github.com/bythos14/kubridge) For fastmem implementation
- @TheIronUniverse for livearea assets
- [dolphin](https://github.com/dolphin-emu/dolphin) For audio stretching code
