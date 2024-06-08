#version 300 es

in vec4 position;
in float oamIndex;

uniform sampler2D oamTex;

out vec3 objPos;

uniform int dispCnt;

int readOam16Aligned(int addr) {
    float x = float(addr >> 2) / 255.0f;
    vec4 value = texture(oamTex, vec2(x, 1.0));
    int entry = addr & 2;
    return int(value[entry] * 255.0) | (int(value[entry + 1] * 255.0) << 8);
}

const vec2 SizeLookup[12] = vec2[12](
vec2(8.0, 8.0), vec2(16.0, 16.0), vec2(32.0, 32.0), vec2(64.0, 64.0), vec2(16.0, 8.0), vec2(32.0, 8.0),
vec2(32.0, 16.0), vec2(64.0, 32.0), vec2(8.0, 16.0), vec2(8.0, 32.0), vec2(16.0, 32.0), vec2(32.0, 64.0)
);

void main() {
    int index = int(oamIndex);

    int attrib0 = readOam16Aligned(index * 8);
    int attrib1 = readOam16Aligned(index * 8 + 2);
    int attrib2 = readOam16Aligned(index * 8 + 4);

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

    objPos = vec3((oamWidth - 0.1) * pos.x, (oamHeight - 0.1) * pos.y, oamIndex);

    float x = float(oamX) + oamWidth * position.x;
    float y = float(oamY) + oamHeight * position.y;

    int priority = (attrib2 >> 10) & 3;
    gl_Position = vec4(x / 256.0 * 2.0 - 1.0, 1.0 - y / 192.0 * 2.0, float(priority) / 5.0, 1.0);
}
