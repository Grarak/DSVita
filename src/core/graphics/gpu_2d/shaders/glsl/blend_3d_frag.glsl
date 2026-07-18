#version 300 es

precision highp int;
precision highp float;

layout(location = 0) out vec4 color;

in vec2 texCoordsBlend;
in vec2 texCoords3d;

uniform highp usampler2D texBlend;
uniform sampler2D tex3d;
uniform sampler2D blendTex;

void main() {
    uvec4 colorBlend = texture(texBlend, texCoordsBlend);
    vec4 color3d = texture(tex3d, texCoords3d);
    vec4 colorBlendF = vec4(colorBlend) / 255.0;

    int mode = int(colorBlend.a);
    switch (mode) {
        case 1: {
            float eva = color3d.a;
            float evb = 1.0 - eva;
            color = vec4(color3d.rgb * eva + colorBlendF.rgb * evb, 1.0);
            break;
        }
        case 2: {
            float bldYF = colorBlendF.r;
            vec3 increaseColor = (1.0 - color3d.rgb) * bldYF;
            color = vec4((color3d.rgb + increaseColor), 1.0);
            break;
        }
        case 3: {
            float bldYF = colorBlendF.r;
            vec3 decreaseColor = color3d.rgb * bldYF;
            color = vec4((color3d.rgb - decreaseColor), 1.0);
            break;
        }
        case 4: {
            color = vec4(color3d.rgb, 1.0);
            break;
        }
        default: {
            color = colorBlendF;
            break;
        }
    }

    // Master brightness: final stage of the engine A output (the blend pass emits an
    // encoded intermediate, so it is applied here); registers come from blendTex row y
    int y = int(texCoordsBlend.y * 191.0);
    vec4 mbRaw = texelFetch(blendTex, ivec2(y, 1), 0);
    int mb = int(mbRaw.r * 255.0) | (int(mbRaw.g * 255.0) << 8);
    int mbFactor = min(mb & 0x1F, 16);
    if (mbFactor != 0) {
        int mbMode = (mb >> 14) & 3;
        float mbF = float(mbFactor) / 16.0;
        if (mbMode == 1) {
            color.rgb += (1.0 - color.rgb) * mbF;
        } else if (mbMode == 2) {
            color.rgb -= color.rgb * mbF;
        }
    }
}
