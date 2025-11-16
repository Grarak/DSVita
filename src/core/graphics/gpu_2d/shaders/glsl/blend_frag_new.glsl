#version 300 es

precision mediump float;
precision highp int;

layout(location = 0) out vec4 color;

uniform BlendUbo {
    int bldCntsAlphasYs[192];
};

uniform sampler2D topLayer;
uniform sampler2D bottomLayer;
uniform sampler2D tex3d;

in vec2 texCoords;

vec3 alphaBlendColors(vec3 topColor, vec3 bottomColor, float eva, float evb) {
    return topColor * eva + bottomColor * evb;
}

void main() {
    int y = int(texCoords.y * 191.0);
    int bldCntAlphaY = bldCntsAlphasYs[y];
    int bldCnt = bldCntAlphaY & 0xFFFF;
    int bldTop = bldCnt & 0xFF;
    int bldBottom = (bldCnt >> 8) & 0xFF;

    int bldMode = (bldCnt >> 6) & 3;

    vec4 topColor = texture(topLayer, texCoords);
    vec4 bottomColor = texture(bottomLayer, texCoords);

    topColor.rgb *= 255.0 / 31.0;
    bottomColor.rgb *= 255.0 / 31.0;

    int topLayer = int(topColor.a * 255.0);
    int bottomLayer = int(bottomColor.a * 255.0);

    int topLayerNum = 5 - ((topLayer >> 3) & 7);
    int bottomLayerNum = 5 - ((bottomLayer >> 3) & 7);

    bool canBe3d = ((bottomLayer >> 6) & 1) != 0;
    if (canBe3d) {
        vec4 color3d = texture(tex3d, vec2(texCoords.x, -texCoords.y + 1.0));
        if (color3d.a > 0.0) {
            int invPrio3d = (topLayer >> 6) & 3;

            int topInvPrio = (topLayer >> 1) & 3;
            if (topLayerNum == 4) {
                topInvPrio += 1;
            }
            if (invPrio3d >= topInvPrio) {
                bool canBlend = ((bldBottom >> topLayerNum) & 1) != 0;
                if (canBlend) {
                    topColor = vec4(alphaBlendColors(color3d.rgb, topColor.rgb, color3d.a, 1.0 - color3d.a).rgb, 1.0);
                    bldMode = 0;
                } else {
                    topColor = color3d;
                    if (bldMode < 2) {
                        bldMode = 0;
                    }
                }
                topLayerNum = 0;
            } else {
                int bottomInvPrio = (bottomLayer >> 1) & 3;
                if (bottomLayerNum == 4) {
                    bottomInvPrio += 1;
                }
                if (invPrio3d >= bottomInvPrio) {
                    bottomColor = color3d;
                    bottomLayerNum = 0;
                }
            }
        }
    }

    int bldEva = (bldCntAlphaY >> 16) & 0x1F;
    int bldEvb = (bldCntAlphaY >> 21) & 0x1F;
    float bldEvaF = float(bldEva) / 16.0;
    float bldEvbF = float(bldEvb) / 16.0;

    bool topOpaque = (topLayer & 1) != 0;
    if (topLayerNum == 4 && !topOpaque) {
        bool canBlend = ((bldBottom >> bottomLayerNum) & 1) != 0;
        if (canBlend) {
            topColor = vec4(alphaBlendColors(topColor.rgb, bottomColor.rgb, bldEvaF, bldEvbF), 1.0);
        } else if (bldMode < 2) {
            bldMode = 0;
        }
    }

    bool canBlendTop = ((bldTop >> topLayerNum) & 1) != 0;
    bool canBlendWin = ((bottomLayer >> 7) & 1) == 0;
    if (bldMode == 0 || !canBlendTop || !canBlendWin) {
        color = vec4(topColor.rgb, 1.0);
    } else if (bldMode == 1) {
        bool canBlendBottom = ((bldBottom >> bottomLayerNum) & 1) != 0;
        if (canBlendBottom) {
            color = vec4(alphaBlendColors(topColor.rgb, bottomColor.rgb, bldEvaF, bldEvbF), 1.0);
        } else {
            color = vec4(topColor.rgb, 1.0);
        }
    } else {
        int bldY = (bldCntAlphaY >> 26) & 0x1F;
        float bldYF = float(bldY) / 16.0;
        if (bldMode == 2) {
            topColor.rgb += (1.0 - topColor.rgb) * bldYF;
        } else {
            topColor.rgb -= topColor.rgb * bldYF;
        }
        color = vec4(topColor.rgb, 1.0);
    }
}
