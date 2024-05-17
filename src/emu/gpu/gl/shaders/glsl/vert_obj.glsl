#version 300 es

in vec4 position;
in float oamIndex;

uniform sampler2D oamTex;

out vec2 screenPos;
flat out ivec2 oamXY;

int readOam8(int addr) {
    float x = float(addr >> 2) / 255.0f;
    return int(texture(oamTex, vec2(x, 1.0))[addr & 3] * 255.0f);
}

int readOam16(int addr) {
    return readOam8(addr) | (readOam8(addr + 1) << 8);
}

void main() {
    int attrib0 = readOam16(int(oamIndex) * 8);
    int attrib1 = readOam16(int(oamIndex) * 8 + 2);
    int attrib2 = readOam16(int(oamIndex) * 8 + 4);

    int oamX = attrib1 & 0x1FF;
    int oamY = attrib0 & 0xFF;

    int shape = (attrib0 >> 12);
    int size = (attrib1 >> 14);

    float oamWidth;
    float oamHeight;
    switch (shape | size) {
        case 0x0: oamWidth =  8.0; oamHeight =  8.0; break;
        case 0x1: oamWidth = 16.0; oamHeight = 16.0; break;
        case 0x2: oamWidth = 32.0; oamHeight = 32.0; break;
        case 0x3: oamWidth = 64.0; oamHeight = 64.0; break;
        case 0x4: oamWidth = 16.0; oamHeight =  8.0; break;
        case 0x5: oamWidth = 32.0; oamHeight =  8.0; break;
        case 0x6: oamWidth = 32.0; oamHeight = 16.0; break;
        case 0x7: oamWidth = 64.0; oamHeight = 32.0; break;
        case 0x8: oamWidth =  8.0; oamHeight = 16.0; break;
        case 0x9: oamWidth =  8.0; oamHeight = 32.0; break;
        case 0xA: oamWidth = 16.0; oamHeight = 32.0; break;
        case 0xB: oamWidth = 32.0; oamHeight = 64.0; break;
        default: oamWidth = 0.0; oamHeight = 0.0; break;
    }

    if (oamX >= 256) {
        oamX -= 512;
    }

    if (oamY >= 192) {
        oamY -= 256;
    }

    float x = float(oamX) + oamWidth * position.x;
    float y = float(oamY) + oamHeight * position.y;
    screenPos = vec2(x, y);
    gl_Position = vec4(x / 255.0 * 2.0 - 1.0, 1.0 - y / 191.0 * 2.0, 0.0, 1.0);
}
