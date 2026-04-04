#version 300 es

in vec2 position;
out vec2 screenPos;
out vec2 screenPosF;
out vec2 screenPosWidescreen;
out vec2 affineDims;

uniform float widescreenInvertCoefficient;

void main() {
    float normX = position.x * 0.5 + 0.5;
    screenPos = vec2(max(normX * 256.0 - 0.1, 0.0), max(position.y - 0.1, 0.0));
    screenPosF = vec2(normX, position.y / 192.0);
    screenPosWidescreen.x = position.x * widescreenInvertCoefficient * 0.5 + 0.5;
    screenPosWidescreen.y = -screenPosF.y;
    screenPosWidescreen.y += 1.0;
    affineDims = vec2(0.0, 0.0);
    gl_Position = vec4(position.x, 1.0 - position.y / 192.0 * 2.0, 0.0, 1.0);
}
