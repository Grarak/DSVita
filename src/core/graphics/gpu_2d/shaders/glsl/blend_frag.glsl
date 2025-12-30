#version 300 es

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
// Priority of 1.0 indicates a discarded fragment
// Due to float imprecision priority must be < 0.9
// Lowest priority is 1.0 / 4.0
float topPrio = 0.9;
float bottomPrio = 0.9;
vec4 topColor = vec4(0.0, 0.0, 0.0, 0.0);
vec4 bottomColor = vec4(0.0, 0.0, 0.0, 0.0);

void sortObjPrio() {
    float prio = texture(objDepthTex, screenPos).r * 2.0 - 1.0;
    if (prio < topPrio) {
        topNum = 4;
        // Give obj a priority boost due to float imprecision
        topPrio = prio - 0.1;
        topColor = texture(objTex, screenPos);
    }
}

void sortBgPrio(int num, sampler2D bgTex) {
    vec4 texColor = texture(bgTex, screenPos);
    if (texColor.a < topPrio) {
        bottomNum = topNum;
        bottomPrio = topPrio;
        bottomColor = topColor;

        topNum = num;
        topPrio = texColor.a;
        topColor = texColor;
    } else if (texColor.a < bottomPrio) {
        bottomNum = num;
        bottomPrio = texColor.a;
        bottomColor = texColor;
    }
}

vec4 alphaBlend(int eva, int evb) {
    float evaF = min(float(eva) / 16.0, 1.0);
    float evbF = min(float(evb) / 16.0, 1.0);
    vec3 blendedColor = topColor.rgb * evaF + bottomColor.rgb * evbF;
    return vec4(blendedColor.rgb, 1.0);
}

void main() {
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

    if (topNum == 4 && topColor.a == 0.0) {
        color = vec4(topColor.rgb, 1.0);
        // Semi transparent object
        bool blendBottom = ((bldCnt >> 8) & (1 << bottomNum)) != 0;
        if (blendBottom) {
            color = alphaBlend(bldEva, bldEvb);
            return;
        }

        int bldMode = (bldCnt >> 6) & 3;
        if (bldMode < 2) {
            color = vec4(topColor.rgb, 1.0);
            return;
        }
    }

    int winEnabled = int(texture(winTex, screenPos).x * 255.0);
    if (((winEnabled >> 5) & 1) == 0) {
        color = vec4(topColor.rgb, 1.0);
        return;
    }

    int bldMode = (bldCnt >> 6) & 3;

    if (bldMode == 0) {
        color = vec4(topColor.rgb, 1.0);
        return;
    }

    bool blendTop = (bldCnt & (1 << topNum)) != 0;
    if (!blendTop) {
        color = vec4(topColor.rgb, 1.0);
        return;
    }

    switch (bldMode) {
        case 1: {
            bool blendBottom = ((bldCnt >> 8) & (1 << bottomNum)) != 0;
            if (!blendBottom) {
                color = vec4(topColor.rgb, 1.0);
                return;
            }
            color = alphaBlend(bldEva, bldEvb);
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
