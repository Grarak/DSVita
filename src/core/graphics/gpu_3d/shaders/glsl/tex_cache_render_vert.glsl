#version 300 es

precision highp int;
precision highp float;

in vec4 position;
in vec3 texCoords;
in vec4 viewport;
in vec4 color;

out vec3 oColor;
out vec2 oTexCoords;
flat out int oPolygonIndex;
flat out int oTextureIndex;

void main() {
    oColor = color.rgb / 31.0;
    oTexCoords = texCoords.xy;
    oPolygonIndex = int(texCoords[2]);
    oTextureIndex = int(color[3]);
    vec2 screenDims = vec2(511.0, 383.0);
    gl_Position = position;
    gl_Position.xy = (gl_Position.w * (viewport.zw + viewport.xy - screenDims) + (viewport.zw - viewport.xy) * gl_Position.xy) / screenDims;
}
