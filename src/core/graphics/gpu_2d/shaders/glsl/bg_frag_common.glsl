#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec3 screenPos;
in vec2 screenPosF;
in vec2 affineDims;

uniform int dispCnt;
uniform int bgCnt;
uniform float bgTexHeight;

uniform BgUbo {
    int bgOfs[192 * 4];
    int bgX[192 * 2];
    int bgY[192 * 2];
    int bgPas[192 * 2];
    int bgPbs[192 * 2];
    int bgPcs[192 * 2];
    int bgPds[192 * 2];
};

uniform sampler2D bgTex;
uniform sampler2D palTex;
uniform sampler2D extPalTex;
uniform sampler2D winTex;
uniform sampler2D display3dTex;

int readBg8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float(addr >> 11) / bgTexHeight;
    return int(texture(bgTex, vec2(x, y))[addr & 3] * 255.0);
}

int readBg16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / bgTexHeight;
    vec4 value = texture(bgTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

int readPal16Aligned(int addr) {
    float x = float(addr >> 2) / 255.0;
    vec4 value = texture(palTex, vec2(x, 1.0));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

int readExtPal16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / 15.0;
    vec4 value = texture(extPalTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

ivec2 calculateAffineCoords(int x, int y, int bgNum) {
    int index = (bgNum - 2) * 192 + y;
    float bgX = float(bgX[index]) / 256.0;
    float bgY = float(bgY[index]) / 256.0;
    float bgPa = float(bgPas[index]) / 256.0;
    float bgPb = float(bgPbs[index]) / 256.0;
    float bgPc = float(bgPcs[index]) / 256.0;
    float bgPd = float(bgPds[index]) / 256.0;
    return ivec2(int(bgX + bgPb + float(x) * bgPa), int(bgY + bgPd + float(x) * bgPc));
}

void setPrio() {
    int priority = bgCnt & 3;
    color.a = float(priority) / 255.0;
}
