#version 300 es

in vec4 position;
in vec2 texCoords;
in vec4 viewport;
in vec4 color;
in vec2 texSize;
in vec4 texModeWeights;

out vec4 oColor;
out vec2 oTexCoords;
out vec4 oTexModeWeights;

void main() {
    oColor = color / 31.0;
    oTexCoords = texCoords / (texSize * 8.0);
    oTexModeWeights = texModeWeights;
    vec2 screenDims = vec2(255.0, 191.0);
    gl_Position = position;
    gl_Position.xy = (gl_Position.w * (viewport.zw + viewport.xy - screenDims) + (viewport.zw - viewport.xy) * gl_Position.xy) / screenDims;
}
