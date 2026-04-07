#version 300 es

in vec2 position;
in vec3 texCoordsColor;
out vec2 texCoords;
flat out float texFactor;

void main() {
    texCoords = texCoordsColor.xy;
    texFactor = texCoordsColor.z;
    gl_Position = vec4(position, 0.0, 1.0);
}
