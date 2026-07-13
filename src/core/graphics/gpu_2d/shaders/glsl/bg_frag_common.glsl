#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec2 screenPos;
in vec2 screenPosF;
in vec2 affineDims;

uniform float dispCntF;
uniform float bgCntF;
uniform float bgTexHeight;
uniform int bgNum;

// std140 + ivec4 packing: the default (shared) block layout is driver-chosen — tight
// int arrays on llvmpipe, 16-byte strides on virgl/v3d, which made the shader read far
// past the uploaded tables (the a64 2d corruption). ivec4 arrays have the same stride
// everywhere under std140 and match the packed Rust-side upload exactly.
layout(std140) uniform BgUbo {
    ivec4 bgOfs[192];
    ivec4 bgX[96];
    ivec4 bgY[96];
    ivec4 bgPas[96];
    ivec4 bgPbs[96];
    ivec4 bgPcs[96];
    ivec4 bgPds[96];
};

int getDispCnt() {
    return floatBitsToInt(dispCntF);
}

int getBgCnt() {
    return floatBitsToInt(bgCntF);
}

// Integer samplers + texelFetch: the old normalized-float coordinates landed on texel
// edges (i/511, y=1.0), so NEAREST sat on the rounding razor — exact on llvmpipe-13,
// off-by-one texels on v3d/llvmpipe-20 (per-pixel garbage). texelFetch is exact by
// construction and the uvec4 result skips the float->int requantization entirely.
uniform highp usampler2D bgTex;
uniform highp usampler2D palTex;
uniform highp usampler2D extPalTex;
uniform sampler2D winTex;
uniform sampler2D display3dTex;

int readBg8(int addr) {
    return int(texelFetch(bgTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0)[addr & 3]);
}

int readBg16Aligned(int addr) {
    uvec4 value = texelFetch(bgTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0);
    int entry = addr & 2;
    return int(value[entry]) | (int(value[entry + 1]) << 8);
}

int readPal16Aligned(int addr) {
    uvec4 value = texelFetch(palTex, ivec2(addr >> 2, 0), 0);
    int entry = addr & 2;
    return int(value[entry]) | (int(value[entry + 1]) << 8);
}

int readExtPal16Aligned(int addr) {
    uvec4 value = texelFetch(extPalTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0);
    int entry = addr & 2;
    return int(value[entry]) | (int(value[entry + 1]) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

ivec2 calculateAffineCoords(int x, int y) {
    int index = (bgNum - 2) * 192 + y;
    float bgX = float(bgX[(index) >> 2][(index) & 3]) / 256.0;
    float bgY = float(bgY[(index) >> 2][(index) & 3]) / 256.0;
    float bgPa = float(bgPas[(index) >> 2][(index) & 3]) / 256.0;
    float bgPb = float(bgPbs[(index) >> 2][(index) & 3]) / 256.0;
    float bgPc = float(bgPcs[(index) >> 2][(index) & 3]) / 256.0;
    float bgPd = float(bgPds[(index) >> 2][(index) & 3]) / 256.0;
    return ivec2(int(bgX + bgPb + float(x) * bgPa), int(bgY + bgPd + float(x) * bgPc));
}

void setPrio() {
    int bgCnt = getBgCnt();
    int priority = bgCnt & 3;
    color.a = float(priority) / 255.0;
}
