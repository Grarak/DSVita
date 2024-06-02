#version 300 es

in vec4 position;
out vec3 screenPos;

uniform int bgCnts[4];
uniform int hOfs[4];
uniform int vOfs[4];

void main() {
    int bgNum = int(position.z);
    int bgCnt = bgCnts[bgNum];

    int priority = bgCnt & 3;

    float xPos = (position.x * 0.5 + 0.5) * 256.0 + float(hOfs[bgNum]);
    xPos = min(xPos, 511.0);
    float yPos = position.y + float(vOfs[bgNum]);
    yPos = min(yPos, 511.0);

    screenPos = vec3(xPos - 0.1, yPos - 0.1, position.z);
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, float(priority + 1) / 5.0, 1.0);
}
