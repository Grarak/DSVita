#version 300 es

in vec4 position;
out vec3 screenPos;
out vec2 screenPosF;
out vec2 affineDims;

uniform int bgCnt;

void main() {
    float size = float(128 << ((bgCnt >> 14) & 0x3));
    affineDims = vec2(size, size);

    float normX = position.x * 0.5 + 0.5;
    screenPos = vec3(max(normX * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0), position.z);
    screenPosF = vec2(normX, position.y / 192.0);
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, 0.0, 1.0);
}
