#version 300 es

in vec4 position;
out vec2 texCoordsBlend;
out vec2 texCoords3d;

uniform float widescreenInvertCoefficient;

void main() {
    texCoordsBlend = vec2(position.z, -position.w + 1.0);
    texCoords3d = texCoordsBlend;
    texCoords3d.x = texCoords3d.x * 2.0 - 1.0;
    texCoords3d.x *= widescreenInvertCoefficient;
    texCoords3d.x = texCoords3d.x * 0.5 + 0.5;
    gl_Position = vec4(position.xy, 0.0, 1.0);
}
