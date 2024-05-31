#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;
in vec2 objPos;
flat in int oOamIndex;
flat in int objBound;
flat in int mapWidth;

uniform sampler2D oamTex;
uniform sampler2D objTex;
uniform sampler2D palTex;

int readOam8(int addr) {
    float x = float(addr >> 2) / 255.0f;
    return int(texture(oamTex, vec2(x, 1.0))[addr & 3] * 255.0f);
}

int readOam16(int addr) {
    return readOam8(addr) | (readOam8(addr + 1) << 8);
}

int readObj8(int addr) {
    float x = float((addr >> 2) & 0x1FF) / 511.0;
    float y = float((addr >> 2) >> 9) / 127.0;
    return int(texture(objTex, vec2(x, y))[addr & 3] * 255.0);
}

int readObj16(int addr) {
    return readObj8(addr) | (readObj8(addr + 1) << 8);
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

void main() {
    int attrib2 = readOam16(oOamIndex * 8 + 4);

    int tileIndex = attrib2 & 0x3FF;
    int tileAddr = tileIndex * int(objBound);

    int palBank = (attrib2 >> 12) & 0xF;
    int palBaseAddr = 0x200 + palBank * 32;

    int objY = int(objPos.y);
    int objX = int(objPos.x);

    tileAddr += ((objY & 7) + (objY >> 3) * int(mapWidth)) * 4;
    tileAddr += (objX >> 3) * 32 + (objX & 7) / 2;

    int palIndex = readObj8(tileAddr);
    palIndex >>= 4 * (objX & 1);
    palIndex &= 0xF;
    if (palIndex == 0) {
        discard;
    }

    int palColor = readPal16(palBaseAddr + palIndex * 2);
    color = vec4(normRgb5(palColor), 1.0);
}
