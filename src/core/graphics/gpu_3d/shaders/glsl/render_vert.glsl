#version 300 es

in vec4 position;
in vec4 color;
in vec2 texCoords;

out vec3 oColor;
out vec2 oTexCoords;
out float oPolygonIndex;

void main() {
    oColor = color.rgb;
    oTexCoords = texCoords;
    oPolygonIndex = color.a;
    gl_Position = vec4(position.xy * position.w, position.z, position.w);
}
