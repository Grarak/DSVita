# DSVita

[![Rust](https://github.com/Grarak/DSVita/actions/workflows/rust.yml/badge.svg)](https://github.com/Grarak/DSVita/actions/workflows/rust.yml)

Experimental NDS Emulator for ARM32/PSVita

## Status

This runs some games, however following things are missing:

- No 3D
  - Some games rely on 3D states (such as Pokemon Diamond), they will get stuck at titlescreen
- No saves
- Incomplete 2D rendering
  - No alpha blending
  - Will crash on unimplemented draw modes
- ARM7 HLE will not work with most games
  - Disable it if certain games don't boot further

## Installation/Setup

- Grab the latest vpk from [releases](https://github.com/Grarak/DSVita/releases)
- Install `libshacccg.suprx`, follow this [guide](https://cimmerian.gitbook.io/vita-troubleshooting-guide/shader-compiler/extract-libshacccg.suprx)
- It's recommend overclock your vita, by default they are unchanged
- Create the folder ux0:dsvita and put your roms there

## Credits

- [NooDS](https://github.com/Hydr8gon/NooDS) was used as reference. A lot of code was taken from there.
- [melonDS](https://github.com/melonDS-emu/melonDS) for ARM7 HLE implementation and jit optimizations.
- [DesmumePSPExperimental](https://github.com/Xiro28/DesmumePSPExperimental) for ARM7 HLE implementation.
- [pokediamond](https://github.com/pret/pokediamond) for ARM7 HLE implementation.
- [DSHBA](https://github.com/DenSinH/DSHBA) Copied some PPU hardware acceleration implementation (Thanks for xiro28 linking me the repo)
- [vitaGL](https://github.com/Rinnegatamante/vitaGL) 2D hardware acceleration wouldn't be possible without it
- [Tonc](https://www.coranac.com/tonc/text/toc.htm) GBA PPU documentation
- [GBATEK](http://problemkaputt.de/gbatek-index.htm) GBA/NDS documentation
- @TheIronUniverse for livearea assets
