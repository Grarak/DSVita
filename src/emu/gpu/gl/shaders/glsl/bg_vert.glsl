#version 300 es

in vec4 position;
out vec3 screenPos;
out vec2 screenPosF;

uniform int bgCnts[4];

void main() {
    int bgNum = int(position.z);
    int bgCnt = bgCnts[bgNum];
    int priority = bgCnt & 3;

    float normX = position.x * 0.5 + 0.5;
    screenPos = vec3(max(normX * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0), position.z);
    screenPosF = vec2(normX, position.y / 192.0);
    // Sprites have a higher priority, so add 0.5 to depth here
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, (float(priority) + 0.5) / 5.0, 1.0);
}
