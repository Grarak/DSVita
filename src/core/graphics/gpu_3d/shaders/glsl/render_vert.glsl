#version 300 es

precision highp int;
precision highp float;

in vec4 position;
in vec3 texCoords;
in vec4 viewport;
in vec3 color;

out vec3 oColor;
out vec2 oTexCoords;
flat out int oPolygonIndex;

void main() {
    oColor = color.rgb / 31.0;
    oTexCoords = texCoords.xy;
    oPolygonIndex = int(texCoords[2]);
    gl_Position = position;
    gl_Position.xy = 255.0 / vec2(255.0, 191.0) * ((viewport.zw - viewport.xy) * (gl_Position.xy + gl_Position.w)) - gl_Position.w;
}
