#version 300 es

in vec4 position;
out vec2 texCoords;

uniform float widthCoefficient;

void main() {
    texCoords = position.zw;
    gl_Position = vec4(position.x * widthCoefficient, position.y, 0.0, 1.0);
}
