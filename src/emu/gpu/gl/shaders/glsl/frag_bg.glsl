#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec2 screenPos;

uniform sampler2D bgTex;
uniform sampler2D palTex;

uniform BgUbo {
    int DispCnt;
    int Cnts[4];
    int HOfs[4];
    int VOfs[4];
};

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

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

vec3 drawText(int x, int y, int bgCnt) {
    int screenAddr = ((DispCnt >> 11) & 0x70000) + ((bgCnt << 3) & 0x0F800);
    int charAddr = ((DispCnt >> 8u) & 0x70000) + ((bgCnt << 12) & 0x3C000);

    x += HOfs[3];
    x &= 0x1FF;
    y += VOfs[3];
    y &= 0x1FF;

    bool is512Width = (bgCnt & (1 << 14)) != 0;
    if (x > 255 && is512Width) {
        screenAddr += 0x800;
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

    bool is8bit = (bgCnt & (1 << 7)) != 0;
    if (is8bit) {
        charAddr += ((screenEntry & 0x3FF) << 6) + (yInBlock << 3);
        charAddr += xInBlock;
    } else {
        charAddr += ((screenEntry & 0x3FF) << 5) + (yInBlock << 2);
        charAddr += xInBlock >> 1;
    }

    int colorIndex = readBg8(charAddr);
    //    return vec3(uvec3(colorIndex & 0xFF, (colorIndex >> 8) & 0xFF, (colorIndex >> 16) & 0xFF)) / 255.0;
    int palAddr = colorIndex;
    if (!is8bit) {
        palAddr >>= 4 * (xInBlock & 1);
        palAddr &= 0xF;
        palAddr += (screenEntry & 0xF000) >> 8;
    }
    palAddr <<= 1;

    int color = readPal16(palAddr);
    return normRgb5(color);
}

void main() {
    int bgCnt = Cnts[3];
    int priority = bgCnt & 3;

    gl_FragDepth = float(priority + 1) / 5.0;
    color = vec4(drawText(int(screenPos.x) + 1, int(screenPos.y), bgCnt), 1.0f);
}
