#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec3 objPos;
flat in ivec2 objDims;
in vec2 screenPosF;
in vec2 objAttrib0Addr;
in vec2 objAttrib2Addr;

uniform int dispCnt;
uniform float objTexHeight;
uniform bool objWindow;

struct ObjAttr {
    int mapWidth;
    int objBounds;
};

uniform ObjUbo {
    ObjAttr objAttrs[256];
};

uniform WinBgUbo {
    int winHV[192 * 2];
    int winInOut[192];
};

uniform sampler2D oamTex;
uniform sampler2D objTex;
uniform sampler2D palTex;
uniform sampler2D extPalTex;
uniform sampler2D winTex;

int readAttrib0() {
    vec4 value = texture(oamTex, objAttrib0Addr);
    return int(value[0] * 255.0) | (int(value[1] * 255.0) << 8);
}

int readAttrib2() {
    vec4 value = texture(oamTex, objAttrib2Addr);
    return int(value[0] * 255.0) | (int(value[1] * 255.0) << 8);
}

int readObj8(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / objTexHeight;
    return int(texture(objTex, vec2(x, y))[addr & 3] * 255.0);
}

int readObj16Aligned(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / objTexHeight;
    vec4 value = texture(objTex, vec2(x, y));
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
    float y = float(addrY) / 3.0;
    vec4 value = texture(extPalTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

vec4 drawSprite(int objX, int objY, int attrib0, int attrib2, ObjAttr attr) {
    int mapWidth = attr.mapWidth;
    int objBound = attr.objBounds;

    int tileIndex = attrib2 & 0x3FF;
    int tileAddr = tileIndex * objBound;
    int tileAddrOffset = ((objY & 7) + (objY >> 3) * mapWidth) * 8;
    tileAddrOffset += (objX >> 3) * 64 + (objX & 7);

    bool is8bpp = ((attrib0 >> 13) & 1) != 0;
    if (!is8bpp) {
        tileAddrOffset /= 2;
    }

    int palIndex = readObj8(tileAddr + tileAddrOffset);
    int palColor;

    if (is8bpp) {
        if (palIndex == 0) {
            discard;
        }

        if (objWindow) {
            int enabled = (winInOut[int(191.0 * screenPosF.y)] >> 24) & 0xFF;
            enabled |= 0x80; // indicate this was set by obj, to avoid win out override
            return vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
        } else {
            bool useExtPal = ((dispCnt >> 31) & 1) != 0;
            if (useExtPal) {
                int palBaseAddr = ((attrib2 >> 12) & 0xF) << 9;
                palColor = readExtPal16Aligned(palBaseAddr + palIndex * 2);
            } else {
                palColor = readPal16Aligned(0x200 + palIndex * 2);
            }
        }
    } else {
        palIndex >>= 4 * (objX & 1);
        palIndex &= 0xF;
        if (palIndex == 0) {
            discard;
        }

        if (objWindow) {
            int enabled = (winInOut[int(191.0 * screenPosF.y)] >> 24) & 0xFF;
            enabled |= 0x80; // indicate this was set by obj, to avoid win out override
            return vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
        } else {
            int palBank = (attrib2 >> 12) & 0xF;
            int palBaseAddr = 0x200 + palBank * 32;
            palColor = readPal16Aligned(palBaseAddr + palIndex * 2);
        }
    }
    return vec4(normRgb5(palColor), 1.0);
}

vec4 drawBitmap(int objX, int objY, ObjAttr attr, int attrib2) {
    int bitmapWidth = attr.mapWidth;
    int dataBase = attr.objBounds;

    int alpha = (attrib2 >> 12) & 0xF;
    if (alpha == 0) {
        discard;
    }
    float alphaF = float(alpha) / 15.0;

    int objColor = readObj16Aligned(dataBase + (objY * bitmapWidth + objX) * 2);
    if (((objColor >> 15) & 1) == 0) {
        discard;
    }
    return vec4(normRgb5(objColor), alphaF);
}

void main() {
    int attrib0 = readAttrib0();
    int attrib2 = readAttrib2();

    if (!objWindow) {
        int winEnabled = int(texture(winTex, screenPosF).x * 255.0);
        if (((winEnabled >> 4) & 1) == 0) {
            discard;
        }
    }

    int objWidth = objDims.x;
    int objHeight = objDims.y;
    int objY = int(objPos.y);
    int objX = int(objPos.x);

    if (objX < 0 || objX >= objWidth || objY < 0 || objY >= objHeight) {
        discard;
    }

    int oamIndex = int(objPos.z);
    ObjAttr attr = objAttrs[oamIndex];

    int gfxMode = (attrib0 >> 10) & 3;
    bool isBitmap = gfxMode == 3;
    if (isBitmap) {
        color = drawBitmap(objX, objY, attr, attrib2);
    } else {
        color = drawSprite(objX, objY, attrib0, attrib2, attr);
        bool semiTransparent = gfxMode == 1;
        if (semiTransparent) {
            color.a = 0.0;
        }
    }
}
