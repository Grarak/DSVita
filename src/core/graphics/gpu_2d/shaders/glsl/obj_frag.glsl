precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec2 objPos;
flat in ivec2 objDims;
in vec2 screenPosF;
flat in int oamAttribBase;

uniform float dispCntF;
uniform float objTexHeight;
uniform bool objWindow;

// std140 + ivec4 packing — see BgUbo in bg_frag_common.glsl.
layout(std140) uniform WinBgUbo {
    ivec4 winHV[96];
    ivec4 winInOut[48];
};

// Integer samplers + texelFetch — see bg_frag_common.glsl.
uniform highp usampler2D oamTex;
uniform highp usampler2D objTex;
uniform highp usampler2D palTex;
uniform highp usampler2D extPalTex;
uniform sampler2D winTex;

int readOam16Aligned(int addr) {
    uvec4 value = texelFetch(oamTex, ivec2(addr >> 2, 0), 0);
    int entry = addr & 2;
    return int(value[entry]) | (int(value[entry + 1]) << 8);
}

int readAttrib0() {
    return readOam16Aligned(oamAttribBase);
}

int readAttrib2() {
    return readOam16Aligned(oamAttribBase + 4);
}

int readObj8(int addr) {
    return int(texelFetch(objTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0)[addr & 3]);
}

int readObj16Aligned(int addr) {
    uvec4 value = texelFetch(objTex, ivec2((addr >> 2) & 0x1FF, addr >> 11), 0);
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

#ifdef BITMAP

vec4 drawBitmap(int objX, int objY, int width, int attrib2) {
    int dispCnt = floatBitsToInt(dispCntF);

    bool objMapping1D = ((dispCnt >> 6) & 1) != 0;
    int bitmapWidth;
    int dataBase;
    if (objMapping1D) {
        bitmapWidth = width;
        int tileIndex = attrib2 & 0x3FF;
        int objBoundary1D = (dispCnt >> 22) & 1;
        dataBase = tileIndex * (128 << objBoundary1D);
    } else {
        int obj2D = (dispCnt >> 5) & 1;
        bitmapWidth = 128 << obj2D;
        int xMask = 0x0F | (obj2D << 4);
        dataBase = (attrib2 & xMask) * 0x10 + (attrib2 & 0x3FF & ~xMask) * 0x80;
    }

    int alpha = (attrib2 >> 12) & 0xF;
    float alphaF = float(alpha) / 15.0;

    int objColor = readObj16Aligned(dataBase + (objY * bitmapWidth + objX) * 2);
    if (((objColor >> 15) & 1) == 0) {
        discard;
    }
    return vec4(normRgb5(objColor), alphaF);
}

#else

vec4 drawSprite(int objX, int objY, int attrib2, int width) {
    int dispCnt = floatBitsToInt(dispCntF);

    bool tile1DMapping = ((dispCnt >> 4) & 1) != 0;
    int mapWidth;
    int objBound;
    if (tile1DMapping) {
        mapWidth = width;
        int obj1DBoundary = (dispCnt >> 20) & 3;
        objBound = 32 << obj1DBoundary;
    } else {
#ifdef BPP8
        mapWidth = 128;
#else
        mapWidth = 256;
#endif
        objBound = 32;
    }

    int tileIndex = attrib2 & 0x3FF;
    int tileAddr = tileIndex * objBound;
    int tileAddrOffset = ((objY & 7) + (objY >> 3) * mapWidth) * 8;
    tileAddrOffset += (objX >> 3) * 64 + (objX & 7);

#ifndef BPP8
    tileAddrOffset /= 2;
#endif

    int palIndex = readObj8(tileAddr + tileAddrOffset);
    int palColor;

#ifdef BPP8
    if (palIndex == 0) {
        discard;
    }

    if (objWindow) {
        int enabled = (winInOut[(int(191.0 * screenPosF.y)) >> 2][(int(191.0 * screenPosF.y)) & 3] >> 24) & 0xFF;
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
#else
    palIndex >>= 4 * (objX & 1);
    palIndex &= 0xF;
    if (palIndex == 0) {
        discard;
    }

    if (objWindow) {
        int enabled = (winInOut[(int(191.0 * screenPosF.y)) >> 2][(int(191.0 * screenPosF.y)) & 3] >> 24) & 0xFF;
        enabled |= 0x80; // indicate this was set by obj, to avoid win out override
        return vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
    } else {
        int palBank = (attrib2 >> 12) & 0xF;
        int palBaseAddr = 0x200 + palBank * 32;
        palColor = readPal16Aligned(palBaseAddr + palIndex * 2);
    }
#endif
    return vec4(normRgb5(palColor), 1.0);
}

#endif

void main() {
    int attrib0 = readAttrib0();
    int attrib2 = readAttrib2();
    int winEnabled = int(texture(winTex, screenPosF).x * 255.0);

#ifdef BITMAP
    bool checkWindow = true;
#else
    bool checkWindow = !objWindow;
#endif
    if (checkWindow && ((winEnabled >> 4) & 1) == 0) {
        discard;
    }

    int objWidth = objDims.x;
    int objHeight = objDims.y;
    int objY = int(objPos.y);
    int objX = int(objPos.x);

    if (objX < 0 || objX >= objWidth || objY < 0 || objY >= objHeight) {
        discard;
    }

#ifdef BITMAP
    color = drawBitmap(objX, objY, objWidth, attrib2);
#else
    color = drawSprite(objX, objY, attrib2, objWidth);
    int gfxMode = (attrib0 >> 10) & 3;
    bool semiTransparent = gfxMode == 1;
    if (semiTransparent) {
        color.a = 0.0;
    }
#endif
}
