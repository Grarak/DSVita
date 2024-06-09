#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec3 screenPos;
in vec2 screenPosF;
in vec2 affineDims;

uniform int dispCnt;
uniform int bgCnts[4];
uniform int bgModes[4];

uniform BgUbo {
    int bgHOfs[192 * 4];
    int bgVOfs[192 * 4];
    float bgX[192 * 2];
    float bgY[192 * 2];
    float bgPas[192 * 2];
    float bgPbs[192 * 2];
    float bgPcs[192 * 2];
    float bgPds[192 * 2];
};

uniform sampler2D bgTex;
uniform sampler2D palTex;
uniform sampler2D extPalTex;
uniform sampler2D winTex;

int readBg8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float((addr >> 2) >> 9) / (BG_TEX_HEIGHT - 1.0);
    return int(texture(bgTex, vec2(x, y))[addr & 3] * 255.0);
}

int readBg16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / (BG_TEX_HEIGHT - 1.0);
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
    int screenEntry = readBg16Aligned(screenAddr);

    int isHFlip = (screenEntry >> 10) & 1;
    int isVFlip = (screenEntry >> 11) & 1;

    xInBlock = abs(isHFlip * 7 - xInBlock);
    yInBlock = abs(isVFlip * 7 - yInBlock);

    bool is8bpp = (bgCnt & (1 << 7)) != 0;
    if (is8bpp) {
        charAddr += ((screenEntry & 0x3FF) << 6) + (yInBlock << 3);
        charAddr += xInBlock;

        int palAddr = readBg8(charAddr);
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;

        bool useExtPal = (dispCnt & (1 << 30)) != 0;
        if (useExtPal) {
            int slot = bgNum < 2 && (bgCnt & (1 << 13)) != 0 ? bgNum + 2 : bgNum;
            palAddr += slot * 8192 + ((screenEntry & 0xF000) >> 3);
            int color = readExtPal16Aligned(palAddr);
            return vec4(normRgb5(color), 1.0);
        } else {
            int color = readPal16Aligned(palAddr);
            return vec4(normRgb5(color), 1.0);
        }
    } else {
        charAddr += ((screenEntry & 0x3FF) << 5) + (yInBlock << 2);
        charAddr += xInBlock >> 1;

        int palAddr = readBg8(charAddr);
        palAddr >>= 4 * (xInBlock & 1);
        palAddr &= 0xF;
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;
        palAddr += (screenEntry & 0xF000) >> 7;

        int color = readPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    }
}

vec4 drawBitmap(int x, int y, int bgNum) {
    int width = int(affineDims.x);
    int height = int(affineDims.y);

    int index = (bgNum - 2) * 192 + y;
    float bgX = bgX[index];
    float bgY = bgY[index];
    float bgPa = bgPas[index];
    float bgPb = bgPbs[index];
    float bgPc = bgPcs[index];
    float bgPd = bgPds[index];

    int bitmapX = int(bgX + bgPb + float(x) * bgPa);
    int bitmapY = int(bgY + bgPd + float(x) * bgPc);

    int bgCnt = bgCnts[bgNum];

    bool wrap = (bgCnt & (1 << 13)) != 0;
    if (wrap) {
        bitmapX &= width - 1;
        bitmapY &= height - 1;
    } else if (bitmapX < 0 || bitmapX >= width || bitmapY < 0 || bitmapY >= height) {
        discard;
    }

    int dataBase = (bgCnt << 6) & 0x7C000;
    bool usePal = (bgCnt & (1 << 2)) == 0;
    if (usePal) {
        int palAddr = readBg8(dataBase + bitmapY * width + bitmapX);
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;

        int color = readPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    } else {
        int color = readBg16Aligned(dataBase + (bitmapY * width + bitmapX) * 2);
        if ((color & (1 << 15)) == 0) {
            discard;
        }
        return vec4(normRgb5(color), 1.0);
    }
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);
    int bgNum = int(screenPos.z);

    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    int mode = bgModes[bgNum];
    switch (mode) {
        case 0: {
            color = drawText(x, y, bgNum);
        }
        break;
        case 2: {
            int bgCnt = bgCnts[bgNum];
            bool isBitmap = (bgCnt & (1 << 7)) != 0;
            if (isBitmap) {
                color = drawBitmap(x, y, bgNum);
            } else {
                discard;
            }
        }
        break;
        default : discard;
    }
}