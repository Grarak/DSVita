#version 300 es

precision highp int;
precision highp float;

layout(location = 0) out vec4 color;

in vec2 texCoordsBlend;
in vec2 texCoords3d;

uniform sampler2D texBlend;
uniform sampler2D tex3d;

void main() {
    vec4 colorBlend = texture(texBlend, texCoordsBlend);
    vec4 color3d = texture(tex3d, texCoords3d);

    int mode = int(colorBlend.a * 255.0);
    switch (mode) {
        case 1: {
            float eva = color3d.a;
            float evb = 1.0 - eva;
            color = vec4(color3d.rgb * eva + colorBlend.rgb * evb, 1.0);
            break;
        }
        case 2: {
            float bldYF = colorBlend.r;
            vec3 increaseColor = (1.0 - color3d.rgb) * bldYF;
            color = vec4((color3d.rgb + increaseColor), 1.0);
            break;
        }
        case 3: {
            float bldYF = colorBlend.r;
            vec3 decreaseColor = color3d.rgb * bldYF;
            color = vec4((color3d.rgb - decreaseColor), 1.0);
            break;
        }
        case 4: {
            color = vec4(color3d.rgb, 1.0);
            break;
        }
        default: {
            color = colorBlend;
            break;
        }
    }
}
