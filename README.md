# DSVita

[![Rust](https://github.com/Grarak/DSVita/actions/workflows/rust.yml/badge.svg)](https://github.com/Grarak/DSVita/actions/workflows/rust.yml)

Fast NDS Emulator for ARM32/PSVita

## Status

[![DSVita Mario Kart](http://img.youtube.com/vi/LS2YlOq5-q0/0.jpg)](https://www.youtube.com/watch?v=LS2YlOq5-q0 "DSVita Mario Kart")

This runs most games, however consider:

- 3D rendering
  - Polygons and their textures are drawn, however no lighting or other shading (e.g. toon) are implemented
  - Games which swap screens every frame for displaying 3D on both screens at the same time, will flicker heavily
- 2D rendering is mostly complete
  - Mosaic and some window objects (you will see black screens or silhouettes) are not implemented
- ARM7 HLE will not work with most games
  - Disable it if certain games don't boot further, get struck, crash or have any issues
  - There are other emulation modes like PartialHle or PartialSoundHle. You can pick them if full HLE breaks anything
- Auto frameskip is always used
  - Games will feel choppy

## Installation/Setup

- Grab the latest vpk from [releases](https://github.com/Grarak/DSVita/releases)
- Install `libshacccg.suprx`, follow this [guide](https://cimmerian.gitbook.io/vita-troubleshooting-guide/shader-compiler/extract-libshacccg.suprx)
- Install `kubridge.skprx` from https://github.com/bythos14/kubridge/releases
- It's strongly recommend to overclock your vita to 500MHz
- Create the folder ux0:data/dsvita and put your roms there

## Credits

- [NooDS](https://github.com/Hydr8gon/NooDS) was used as reference. A lot of code was taken from there.
- [melonDS](https://github.com/melonDS-emu/melonDS) for ARM7 HLE implementation and jit optimizations.
- [DesmumePSPExperimental](https://github.com/Xiro28/DesmumePSPExperimental) for ARM7 HLE implementation.
- [pokediamond](https://github.com/pret/pokediamond) for ARM7 HLE implementation.
- [DSHBA](https://github.com/DenSinH/DSHBA) Copied some PPU hardware acceleration implementation (Thanks for xiro28 linking me the repo)
- [vitaGL](https://github.com/Rinnegatamante/vitaGL) 2D/3D hardware acceleration wouldn't be possible without it
- [Tonc](https://www.coranac.com/tonc/text/toc.htm) GBA PPU documentation
- [GBATEK](http://problemkaputt.de/gbatek-index.htm) GBA/NDS documentation
- [kubridge](https://github.com/bythos14/kubridge) For fastmem implementation
- @TheIronUniverse for livearea assets
