#version 300 es

precision highp float;
precision highp int;

in vec4 position;
in float oamIndex;

uniform sampler2D oamTex;

out vec3 objPos;
flat out ivec2 objDims;
out vec2 screenPosF;
out vec2 objAttrib0Addr;
out vec2 objAttrib2Addr;

uniform int dispCnt;
uniform bool objWindow;

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

float fixedToFloat(int n) {
    bool sign = (n & (1 << 15)) != 0;
    if (sign) {
        n -= 1;
        n = ~n;
        n &= 0xFFFF;
        float f = float(n) / 256.0;
        return f * -1.0;
    } else {
        float f = float(n) / 256.0;
        return f;
    }
}

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
    objDims = ivec2(int(oamWidth), int(oamHeight));

    if (oamX >= 256) {
        oamX -= 512;
    }

    if (oamY >= 192) {
        oamY -= 256;
    }

    vec2 pos = position.xy;

    bool affine = ((attrib0 >> 8) & 1) != 0;
    if (affine) {
        int affineIndex = (attrib1 >> 9) & 0x1F;
        int affineOffset = affineIndex * 0x20;
        int pa = readOam16Aligned(affineOffset + 6);
        int pb = readOam16Aligned(affineOffset + 14);
        int pc = readOam16Aligned(affineOffset + 22);
        int pd = readOam16Aligned(affineOffset + 30);
        mat2 m = mat2(fixedToFloat(pa), fixedToFloat(pb), fixedToFloat(pc), fixedToFloat(pd));

        bool doubleAffine = (attrib0 & (1 << 9)) != 0;
        if (doubleAffine) {
            vec2 normPos = vec2(max(oamWidth * 2.0 * pos.x - 0.9, 0.0) - oamWidth, max(oamHeight * 2.0 * pos.y - 0.9, 0.0) - oamHeight) * m;
            objPos = vec3(normPos.x + oamWidth / 2.0, normPos.y + oamHeight / 2.0, oamIndex);
            oamWidth *= 2.0;
            oamHeight *= 2.0;
        } else {
            vec2 normPos = vec2(max(oamWidth * pos.x - 0.5, 0.0) - oamWidth / 2.0, max(oamHeight * pos.y - 0.5, 0.0) - oamHeight / 2.0) * m;
            objPos = vec3(normPos.x + oamWidth / 2.0 - 0.5, normPos.y + oamHeight / 2.0, oamIndex);
        }
    } else {
        bool isVFlip = ((attrib1 >> 13) & 1) != 0;
        bool isHFlip = ((attrib1 >> 12) & 1) != 0;

        if (isVFlip) {
            pos.y = -pos.y + 1.0;
        }

        if (isHFlip) {
            pos.x = -pos.x + 1.0;
        }

        objPos = vec3((oamWidth - 0.1) * pos.x, (oamHeight - 0.1) * pos.y, oamIndex);
    }

    float x = float(oamX) + oamWidth * position.x;
    float y = float(oamY) + oamHeight * position.y;

    screenPosF = vec2(x / 255.0, y / 191.0);
    objAttrib0Addr = vec2(oamIndex * 8.0 / 4.0 / 255.0, 1.0);
    objAttrib2Addr = vec2((oamIndex * 8.0 + 4.0) / 4.0 / 255.0, 1.0);

    int priority = (attrib2 >> 10) & 3;
    gl_Position = vec4(screenPosF.x * 2.0 - 1.0, 1.0 - screenPosF.y * 2.0, float(priority) / 4.0, 1.0);
    if (objWindow) {
        gl_Position.y = -gl_Position.y;
    }
}
