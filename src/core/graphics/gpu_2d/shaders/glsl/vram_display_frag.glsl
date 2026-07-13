#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec2 screenPos;
uniform float dispCntF;

// Integer sampler + texelFetch — see bg_frag_common.glsl.
uniform highp usampler2D lcdcPalTex;

int readLcdcPal16Aligned(int addr) {
    uvec4 value = texelFetch(lcdcPalTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0);
    int entry = addr & 2;
    return int(value[entry]) | (int(value[entry + 1]) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);

    int dispCnt = floatBitsToInt(dispCntF);
    int addr = ((dispCnt >> 18) & 0x3) * 0x10000 + y * 256 + x;
    color = vec4(normRgb5(readLcdcPal16Aligned(addr * 2)), 0.0);
}
