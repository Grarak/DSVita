#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec3 screenPos;

uniform int dispCnt;
uniform int bgCnts[4];

uniform BgUbo {
    int bgHOfs[192 * 4];
    int bgVOfs[192 * 4];
};

uniform sampler2D bgTex;
uniform sampler2D palTex;
uniform sampler2D winTex;

int readBg8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float((addr >> 2) >> 9) / 255.0;
    return int(texture(bgTex, vec2(x, y))[addr & 3] * 255.0);
}

int readBg16(int addr) {
    return readBg8(addr) | (readBg8(addr + 1) << 8);
}

int readPal8(int addr) {
    float x = float(addr >> 2) / 255.0;
    return int(texture(palTex, vec2(x, 1.0))[addr & 3] * 255.0);
}

int readPal16(int addr) {
    return readPal8(addr) | (readPal8(addr + 1) << 8);
}

int readWin(int x, int y) {
    return int(texture(winTex, vec2(float(x) / 255.0, float(y) / 192.0)).x * 255.0);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

vec4 drawText(int x, int y, int bgNum) {
    int bgCnt = bgCnts[bgNum];

    int screenAddr = ((dispCnt >> 11) & 0x70000) + ((bgCnt << 3) & 0x0F800);
    int charAddr = ((dispCnt >> 8u) & 0x70000) + ((bgCnt << 12) & 0x3C000);

    x += bgHOfs[bgNum * 192 + y];
    x &= 0x1FF;
    y += bgVOfs[bgNum * 192 + y];
    y &= 0x1FF;

    // 512 Width
    if (x > 255 && (bgCnt & (1 << 14)) != 0) {
        screenAddr += 0x800;
    }

    // 512 Height
    if (y > 255 && (bgCnt & (1 << 15)) != 0) {
        screenAddr += (bgCnt & (1 << 14)) != 0 ? 0x1000 : 0x800;
    }

    int xBlock = x & 0xF8;
    int xInBlock = x & 7;
    int yBlock = y & 0xF8;
    int yInBlock = y & 7;

    screenAddr += yBlock << 3;
    screenAddr += xBlock >> 2;
    int screenEntry = readBg16(screenAddr);

    int isHFlip = (screenEntry >> 10) & 1;
    int isVFlip = (screenEntry >> 11) & 1;

    xInBlock = abs(isHFlip * 7 - xInBlock);
    yInBlock = abs(isVFlip * 7 - yInBlock);

    int palBaseAddr;
    bool is8bpp = (bgCnt & (1 << 7)) != 0;
    if (is8bpp) {
        charAddr += ((screenEntry & 0x3FF) << 6) + (yInBlock << 3);
        charAddr += xInBlock;
        palBaseAddr = 0;
    } else {
        charAddr += ((screenEntry & 0x3FF) << 5) + (yInBlock << 2);
        charAddr += xInBlock >> 1;
        palBaseAddr = (screenEntry & 0xF000) >> 8;
    }

    int palAddr = readBg8(charAddr);
    if (!is8bpp) {
        palAddr >>= 4 * (xInBlock & 1);
        palAddr &= 0xF;
    }
    if (palAddr == 0) {
        discard;
    }
    palAddr += palBaseAddr;
    palAddr <<= 1;

    int color = readPal16(palAddr);
    return vec4(normRgb5(color), 1.0);
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);
    int bgNum = int(screenPos.z);

    int winEnabled = readWin(x, y);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    color = drawText(x, y, bgNum);
}
