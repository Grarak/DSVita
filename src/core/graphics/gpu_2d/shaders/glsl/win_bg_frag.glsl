#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec2 screenPos;
in vec2 screenPosF;
uniform int dispCnt;

uniform WinBgUbo {
    int winHV[192 * 2];
    int winInOut[192];
};

uniform sampler2D objWinTex;

bool checkBounds(int x, int y, int winNum) {
    bool winEnabled = (dispCnt & (1 << (13 + winNum))) != 0;
    if (!winEnabled) {
        return false;
    }

    int hv = winHV[y * 2 + winNum];
    int h = hv & 0xFFFF;
    int v = (hv >> 16) & 0xFFFF;

    int winX1 = (h >> 8) & 0xFF;
    int winX2 = h & 0xFF;

    int winY1 = (v >> 8) & 0xFF;
    int winY2 = v & 0xFF;

    if (winX1 <= winX2) {
        if (x < winX1 || x > winX2) {
            return false;
        }
    } else {
        if (x >= winX2 && x < winX1) {
            return false;
        }
    }

    if (winY1 <= winY2) {
        if (y < winY1 || y > winY2) {
            return false;
        }
    } else {
        if (y >= winY2 && y < winY1) {
            return false;
        }
    }

    int winIn = winInOut[y] & 0xFFFF;
    int enabled = (winIn >> (winNum * 8)) & 0xFF;
    color = vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
    return true;
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);
    int objWin = int(texture(objWinTex, screenPosF).x * 255.0);

    if (!checkBounds(x, y, 0) && !checkBounds(x, y, 1)) {
        if (((objWin >> 7) & 1) != 0) {
            objWin &= 0x7F;
            color = vec4(float(objWin) / 255.0, 0.0, 0.0, 0.0);
        } else {
            int enabled = (winInOut[y] >> 16) & 0xFF;
            color = vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
        }
    }
}
