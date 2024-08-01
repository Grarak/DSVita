#version 300 es

in vec4 position;
out vec3 screenPos;
out vec2 screenPosF;
out vec2 affineDims;

uniform int bgCnt;

const vec2 BitMapSizeLookup[4] = vec2[4](vec2(128.0, 128.0), vec2(256.0, 256.0), vec2(512.0, 256.0), vec2(512.0, 512.0));

void main() {
    int size = (bgCnt >> 14) & 0x3;
    affineDims = BitMapSizeLookup[size];

    float normX = position.x * 0.5 + 0.5;
    screenPos = vec3(max(normX * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0), position.z);
    screenPosF = vec2(normX, position.y / 192.0);
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, 0.0, 1.0);
}
