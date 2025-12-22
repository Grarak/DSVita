#version 300 es

precision highp int;
precision highp float;

in vec4 position;
in vec4 color;
in vec2 texCoords;

out vec3 oColor;
out vec2 oTexCoords;
out vec2 texImageParamAddr;
out vec2 palPolyAttribAddr;

void main() {
    oColor = color.rgb;
    oTexCoords = texCoords;

    int polygonIndex = int(color.a);
    polygonIndex <<= 1;
    float x = float(polygonIndex & 0x7F) / 127.0;
    float y = float(polygonIndex >> 7) / 127.0;
    texImageParamAddr = vec2(x, y);

    polygonIndex += 1;
    x = float(polygonIndex & 0x7F) / 127.0;
    y = float(polygonIndex >> 7) / 127.0;
    palPolyAttribAddr = vec2(x, y);

    gl_Position = vec4(position.x / (64.0 * 256.0), position.y / (64.0 * 192.0), position.zw / 4096.0);
}
