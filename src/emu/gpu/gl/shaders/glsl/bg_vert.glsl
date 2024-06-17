#version 300 es

in vec4 position;
out vec3 screenPos;
out vec2 screenPosF;
out vec2 affineDims;

uniform int bgCnt;
uniform int bgMode;

const vec2 BitMapSizeLookup[4] = vec2[4](vec2(128.0, 128.0), vec2(256.0, 256.0), vec2(512.0, 256.0), vec2(512.0, 512.0));

void main() {
    // Extended Affine background
    if (bgMode == 2) {
        bool isBitMap = (bgCnt & (1 << 7)) != 0;
        if (isBitMap) {
            int size = (bgCnt >> 14) & 0x3;
            affineDims = BitMapSizeLookup[size];
        } else {
            float size = float(128 << ((bgCnt >> 14) & 0x3));
            affineDims = vec2(size, size);
        }
    }

    float normX = position.x * 0.5 + 0.5;
    screenPos = vec3(max(normX * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0), position.z);
    screenPosF = vec2(normX, position.y / 192.0);
    // Sprites have a higher priority, so add 0.5 to depth here
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, 0.0, 1.0);
}
