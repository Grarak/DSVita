#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec2 screenPos;
uniform int dispCnt;

uniform sampler2D lcdcPalTex;

int readLcdcPal16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / 327.0;
    vec4 value = texture(lcdcPalTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);

    int addr = ((dispCnt >> 18) & 0x3) * 0x10000 + y * 256 + x;
    color = vec4(normRgb5(readLcdcPal16Aligned(addr * 2)), 0.0);
}
