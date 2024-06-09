# DSPSV

[![Rust](https://github.com/Grarak/DSPSV/actions/workflows/rust.yml/badge.svg)](https://github.com/Grarak/DSPSV/actions/workflows/rust.yml)

Experimental NDS Emulator for ARM32/PSVita

## Status

This runs some games, however following things are missing:

- No 3D
- No saves
- Incomplete 2D rendering
  - No alpha blending
  - Will crash on unimplemented draw modes

## Credits

- [NooDS](https://github.com/Hydr8gon/NooDS) was used as reference. A lot of code was taken from there.
- [melonDS](https://github.com/melonDS-emu/melonDS) for the ARM7 HLE implementation.
- [DesmumePSPExperimental](https://github.com/Xiro28/DesmumePSPExperimental) Was also helpful for ARM7 HLE implementation.
- [DSHBA](https://github.com/DenSinH/DSHBA) Copied some PPU hardware acceleration implementation (Thanks for xiro28 linking me the repo)
- [vitaGL](https://github.com/Rinnegatamante/vitaGL) 2D hardware acceleration wouldn't be possible without it
- [Tonc](https://www.coranac.com/tonc/text/toc.htm) GBA PPU documentation
- [GBATEK](http://problemkaputt.de/gbatek-index.htm) GBA/NDS documentation
- @flow3731 for livearea assets
