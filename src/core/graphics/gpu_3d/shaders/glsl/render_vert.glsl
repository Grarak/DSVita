#version 300 es

in vec4 position;
in vec3 color;
in vec2 texCoords;

out vec3 oColor;
out vec2 oTexCoords;
out float oPolygonIndex;

void main() {
    oColor = color;
    oTexCoords = texCoords;
    oPolygonIndex = position.w;
    gl_Position = vec4(position.xyz, 1.0);
}
