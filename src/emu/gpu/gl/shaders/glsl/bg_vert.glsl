#version 300 es

in vec4 position;
out vec3 screenPos;

uniform int bgCnts[4];

void main() {
    int bgNum = int(position.z);
    int bgCnt = bgCnts[bgNum];
    int priority = bgCnt & 3;

    screenPos = vec3(max((position.x * 0.5 + 0.5) * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0), position.z);
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, float(priority + 1) / 5.0, 1.0);
}
