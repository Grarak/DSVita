precision highp int;
precision highp float;

layout(location = 0) out vec4 color;

in vec2 screenPos;

uniform sampler2D bg0Tex;
uniform sampler2D bg1Tex;
uniform sampler2D bg2Tex;
uniform sampler2D bg3Tex;
uniform sampler2D objTex;
uniform sampler2D objDepthTex;
uniform sampler2D winTex;

uniform BlendUbo {
    int bldCntsAlphasYs[192];
};

int topNum = 5;
int bottomNum = 5;
#ifdef BLEND_3D
bool top3D = false;
#endif
int topPrio = 4;
int bottomPrio = 4;
vec4 topColor = vec4(0.0, 0.0, 0.0, 0.0);
vec4 bottomColor = vec4(0.0, 0.0, 0.0, 0.0);

void sortObjPrio() {
    int prio = int((texture(objDepthTex, screenPos).r * 2.0 - 1.0) * 4.0);
    if (prio < topPrio) {
        topNum = 4;
        topPrio = prio;
        topColor = texture(objTex, screenPos);
    }
}

void sortBgPrio(int num, sampler2D bgTex) {
    vec4 texColor = texture(bgTex, screenPos);
    int data = int(texColor.a * 255.0);
    if (data == 255) { // frag was discarded
        return;
    }

    int prio = data & 0x3;
    if (prio < topPrio) {
        bottomNum = topNum;
        bottomPrio = topPrio;
        bottomColor = topColor;

        topNum = num;
        topPrio = prio;
        topColor = texColor;
#ifdef BLEND_3D
        int alpha3D = (data >> 2) & 0x3F;
        top3D = num == 0 && alpha3D != 0;
        if (top3D) {
            topColor.a = float(alpha3D) / 31.0;
        }
#endif
    } else if (prio < bottomPrio) {
        bottomNum = num;
        bottomPrio = prio;
        bottomColor = texColor;
    }
}

vec4 alphaBlend(int eva, int evb) {
    float evaF = float(eva) / 16.0;
    float evbF = float(evb) / 16.0;
    vec4 blendedColor = vec4(topColor.rgb * evaF + bottomColor.rgb * evbF, 1.0);
    return blendedColor;
}

void main() {
    int winEnabled = int(texture(winTex, screenPos).x * 255.0);

    sortObjPrio();
    sortBgPrio(0, bg0Tex);
    sortBgPrio(1, bg1Tex);
    sortBgPrio(2, bg2Tex);
    sortBgPrio(3, bg3Tex);

    if (topNum == 5) {
        discard;
    }

    int y = int(screenPos.y * 191.0);
    int bldCntAlphaY = bldCntsAlphasYs[y];
    int bldCnt = bldCntAlphaY & 0xFFFF;
    int bldEva = (bldCntAlphaY >> 16) & 0x1F;
    int bldEvb = (bldCntAlphaY >> 21) & 0x1F;
    int bldY = (bldCntAlphaY >> 26) & 0x1F;
    bool blendTop = ((bldCnt >> topNum) & 1) != 0;
    bool blendBottom = ((bldCnt >> (8 + bottomNum)) & 1) != 0;
    int bldMode = (bldCnt >> 6) & 3;

#ifdef BLEND_3D
    if (top3D) {
        if (blendBottom) {
            color = vec4(bottomColor.rgb, 1.0 / 255.0);
        } else if (bldMode < 2 || !blendTop || ((winEnabled >> 5) & 1) == 0) {
            color = vec4(0.0, 0.0, 0.0, 4.0 / 255.0);
        } else {
            switch (bldMode) {
                case 2: {
                    float bldYF = float(bldY) / 16.0;
                    color = vec4(bldYF, 0.0, 0.0, 2.0 / 255.0);
                    break;
                }
                case 3: {
                    float bldYF = float(bldY) / 16.0;
                    color = vec4(bldYF, 0.0, 0.0, 3.0 / 255.0);
                    break;
                }
            }
        }
        return;
    } else
#endif
    if (topNum == 4 && topColor.a == 0.0) {
        // Semi transparent object
        if (blendBottom) {
            color = alphaBlend(bldEva, bldEvb);
            return;
        }

        if (bldMode < 2) {
            bldMode = 0;
        }
    }

    if (bldMode == 0 || !blendTop || ((winEnabled >> 5) & 1) == 0) {
        color = vec4(topColor.rgb, 1.0);
    } else {
        switch (bldMode) {
            case 1: {
                if (blendBottom) {
                    color = alphaBlend(bldEva, bldEvb);
                } else {
                    color = vec4(topColor.rgb, 1.0);
                }
                break;
            }
            case 2: {
                float bldYF = float(bldY) / 16.0;
                vec3 increaseColor = (1.0 - topColor.rgb) * bldYF;
                color = vec4((topColor.rgb + increaseColor), 1.0);
                break;
            }
            case 3: {
                float bldYF = float(bldY) / 16.0;
                vec3 decreaseColor = topColor.rgb * bldYF;
                color = vec4((topColor.rgb - decreaseColor), 1.0);
                break;
            }
        }
    }
}
