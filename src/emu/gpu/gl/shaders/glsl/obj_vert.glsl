#version 300 es

in vec4 position;
in float oamIndex;

uniform sampler2D oamTex;

out vec2 objPos;
flat out int oOamIndex;
flat out int objBound;
flat out int mapWidth;

uniform int dispCnt;

int readOam8(int addr) {
    float x = float(addr >> 2) / 255.0f;
    return int(texture(oamTex, vec2(x, 1.0))[addr & 3] * 255.0f);
}

int readOam16(int addr) {
    return readOam8(addr) | (readOam8(addr + 1) << 8);
}

const vec2 SizeLookup[12] = vec2[12](
    vec2(8.0, 8.0), vec2(16.0, 16.0), vec2(32.0, 32.0), vec2(64.0, 64.0), vec2(16.0, 8.0), vec2(32.0, 8.0),
    vec2(32.0, 16.0), vec2(64.0, 32.0), vec2(8.0, 16.0), vec2(8.0, 32.0), vec2(16.0, 32.0), vec2(32.0, 64.0)
);

void main() {
    int attrib0 = readOam16(int(oamIndex) * 8);
    int attrib1 = readOam16(int(oamIndex) * 8 + 2);
    int attrib2 = readOam16(int(oamIndex) * 8 + 4);

    int oamX = attrib1 & 0x1FF;
    int oamY = attrib0 & 0xFF;

    int shape = (attrib0 >> 12) & 0xC;
    int size = (attrib1 >> 14) & 0x3;

    vec2 oamDims = SizeLookup[shape | size];
    float oamWidth = oamDims.x;
    float oamHeight = oamDims.y;

    if (oamX >= 256) {
        oamX -= 512;
    }

    if (oamY >= 192) {
        oamY -= 256;
    }

    bool is1dMap = (dispCnt & (1 << 4)) != 0;
    objBound = 32 << (int(is1dMap) * ((dispCnt >> 20) & 0x3));

    bool isVFlip = (attrib1 & (1 << 13)) != 0;
    bool isHFlip = (attrib1 & (1 << 12)) != 0;

    vec2 pos = position.xy;
    if (isVFlip) {
        pos.y -= 1.0;
        pos.y = abs(pos.y);
    }

    if (isHFlip) {
        pos.x -= 1.0;
        pos.x = abs(pos.x);
    }

    objPos = vec2((oamWidth - 0.1) * pos.x, (oamHeight - 0.1) * pos.y);
    oOamIndex = int(oamIndex);
    mapWidth = is1dMap ? int(oamWidth) : 256;

    float x = float(oamX) + oamWidth * position.x;
    float y = float(oamY) + oamHeight * position.y;

    int priority = (attrib2 >> 10) & 3;
    gl_Position = vec4(x / 256.0 * 2.0 - 1.0, 1.0 - y / 192.0 * 2.0, float(priority) / 5.0, 1.0);
}
