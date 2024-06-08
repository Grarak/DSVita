#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec3 objPos;

uniform int dispCnt;
uniform ObjUbo {
    int mapWidths[128];
    int objBounds[128];
};

uniform sampler2D oamTex;
uniform sampler2D objTex;
uniform sampler2D palTex;
uniform sampler2D extPalTex;

int readOam16Aligned(int addr) {
    float x = float(addr >> 2) / 255.0f;
    vec4 value = texture(oamTex, vec2(x, 1.0));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

int readObj8(int addr) {
    int addrX = (addr >> 2) & 0x1FF;
    int addrY = addr >> 11;
    float x = float(addrX) / 511.0;
    float y = float(addrY) / (OBJ_TEX_HEIGHT - 1.0);
    return int(texture(objTex, vec2(x, y))[addr & 3] * 255.0);
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
    float y = float(addrY) / 7.0;
    vec4 value = texture(extPalTex, vec2(x, y));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

vec3 normRgb5(int color) {
    return vec3(float(color & 0x1F), float((color >> 5) & 0x1F), float((color >> 10) & 0x1F)) / 31.0;
}

void main() {
    int oamIndex = int(objPos.z);

    int attrib0 = readOam16Aligned(oamIndex * 8);
    int attrib2 = readOam16Aligned(oamIndex * 8 + 4);

    int mapWidth = int(mapWidths[oamIndex]);
    int objBound = int(objBounds[oamIndex]);

    int tileIndex = attrib2 & 0x3FF;
    int tileAddr = tileIndex * objBound;

    int objY = int(objPos.y);
    int objX = int(objPos.x);

    bool is8bpp = (attrib0 & (1 << 13)) != 0;
    if (is8bpp) {
        tileAddr += ((objY & 7) + (objY >> 3) * mapWidth) * 8;
        tileAddr += (objX >> 3) * 64 + (objX & 7);

        int palIndex = readObj8(tileAddr);
        if (palIndex == 0) {
            discard;
        }

        bool useExtPal = (dispCnt & (1 << 31)) != 0;
        if (useExtPal) {
            int palBaseAddr = (attrib2 & 0xF000) >> 3;
            int palColor = readExtPal16Aligned(palBaseAddr + palIndex * 2);
            color = vec4(normRgb5(palColor), 1.0);
        } else {
            int palColor = readPal16Aligned(0x200 + palIndex * 2);
            color = vec4(normRgb5(palColor), 1.0);
        }
    } else {
        tileAddr += ((objY & 7) + (objY >> 3) * mapWidth) * 4;
        tileAddr += (objX >> 3) * 32 + (objX & 7) / 2;

        int palIndex = readObj8(tileAddr);
        palIndex >>= 4 * (objX & 1);
        palIndex &= 0xF;
        if (palIndex == 0) {
            discard;
        }

        int palBank = (attrib2 >> 12) & 0xF;
        int palBaseAddr = 0x200 + palBank * 32;
        int palColor = readPal16Aligned(palBaseAddr + palIndex * 2);
        color = vec4(normRgb5(palColor), 1.0);
    }
}
