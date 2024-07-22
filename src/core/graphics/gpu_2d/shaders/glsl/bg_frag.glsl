#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec3 screenPos;
in vec2 screenPosF;
in vec2 affineDims;

uniform int dispCnt;
uniform int bgCnt;
uniform int bgMode;

uniform BgUbo {
    int bgOfs[192 * 4];
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
uniform sampler2D display3dTex;

int readBg8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float(addr >> 11) / (BG_TEX_HEIGHT - 1.0);
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
    int screenAddr = ((dispCnt >> 11) & 0x70000) + ((bgCnt << 3) & 0x0F800);
    int charAddr = ((dispCnt >> 8) & 0x70000) + ((bgCnt << 12) & 0x3C000);

    int of = bgOfs[bgNum * 192 + y];
    x += of & 0xFFFF;
    x &= 0x1FF;
    y += of >> 16;
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

ivec2 calculateAffineCoords(int x, int y, int bgNum) {
    int index = (bgNum - 2) * 192 + y;
    float bgX = bgX[index];
    float bgY = bgY[index];
    float bgPa = bgPas[index];
    float bgPb = bgPbs[index];
    float bgPc = bgPcs[index];
    float bgPd = bgPds[index];
    return ivec2(int(bgX + bgPb + float(x) * bgPa), int(bgY + bgPd + float(x) * bgPc));
}

vec4 drawBitmap(int x, int y, int bgNum) {
    int width = int(affineDims.x);
    int height = int(affineDims.y);

    ivec2 coords = calculateAffineCoords(x, y, bgNum);

    bool wrap = (bgCnt & (1 << 13)) != 0;
    if (wrap) {
        coords.x &= width - 1;
        coords.y &= height - 1;
    } else if (coords.x < 0 || coords.x >= width || coords.y < 0 || coords.y >= height) {
        discard;
    }

    int dataBase = (bgCnt << 6) & 0x7C000;
    bool usePal = (bgCnt & (1 << 2)) == 0;
    if (usePal) {
        int palAddr = readBg8(dataBase + coords.y * width + coords.x);
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;

        int color = readPal16Aligned(palAddr);
        return vec4(normRgb5(color), 1.0);
    } else {
        int color = readBg16Aligned(dataBase + (coords.y * width + coords.x) * 2);
        if ((color & (1 << 15)) == 0) {
            discard;
        }
        return vec4(normRgb5(color), 1.0);
    }
}

vec4 drawAffine(int x, int y, int bgNum, bool extended) {
    int size = int(affineDims.x);

    ivec2 coords = calculateAffineCoords(x, y, bgNum);

    bool wrap = (bgCnt & (1 << 13)) != 0;
    if (wrap) {
        coords.x &= size - 1;
        coords.y &= size - 1;
    } else if (coords.x < 0 || coords.x >= size || coords.y < 0 || coords.y >= size) {
        discard;
    }

    int screenAddr = ((dispCnt >> 11) & 0x70000) + ((bgCnt << 3) & 0x0F800);
    int charAddr = ((dispCnt >> 8) & 0x70000) + ((bgCnt << 12) & 0x3C000);

    int xBlockNum = coords.x >> 3;
    int xInBlock = coords.x & 7;
    int yBlockNum = coords.y >> 3;
    int yInBlock = coords.y & 7;

    if (extended) {
        screenAddr += (yBlockNum * (size / 8) + xBlockNum) * 2;
        int screenEntry = readBg16Aligned(screenAddr);

        int isHFlip = (screenEntry >> 10) & 1;
        int isVFlip = (screenEntry >> 11) & 1;

        xInBlock = abs(isHFlip * 7 - xInBlock);
        yInBlock = abs(isVFlip * 7 - yInBlock);

        charAddr += (screenEntry & 0x3FF) * 64 + yInBlock * 8 + xInBlock;

        int palAddr = readBg8(charAddr);
        if (palAddr == 0) {
            discard;
        }
        palAddr *= 2;

        bool useExtPal = (dispCnt & (1 << 30)) != 0;
        if (useExtPal) {
            palAddr += bgNum * 8192 + ((screenEntry & 0xF000) >> 3);
            int color = readExtPal16Aligned(palAddr);
            return vec4(normRgb5(color), 1.0);
        } else {
            int color = readPal16Aligned(palAddr);
            return vec4(normRgb5(color), 1.0);
        }
    } else {
        discard;
    }
}

void main() {
    int bgNum = int(screenPos.z);

    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
    if ((winEnabled & (1 << bgNum)) == 0) {
        discard;
    }

    int x = int(screenPos.x);
    int y = int(screenPos.y);

    switch (bgMode) {
        case 0: {
            color = drawText(x, y, bgNum);
            break;
        }
        case 2: {
            bool isBitmap = (bgCnt & (1 << 7)) != 0;
            if (isBitmap) {
                color = drawBitmap(x, y, bgNum);
            } else {
                color = drawAffine(x, y, bgNum, true);
            }
            break;
        }
        case 4: {
            color = texture(display3dTex, vec2(screenPosF.x, 1.0 - screenPosF.y));
            if (color.a == 0.0) {
                discard;
            }
            break;
        }
        default : discard;
    }

    int priority = bgCnt & 3;
    color.a = float(priority) / 4.0;
}
