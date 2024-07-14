#version 300 es

precision highp float;
precision highp int;

layout(location = 0) out vec4 color;

in vec2 screenPos;
uniform int dispCnt;

uniform WinBgUbo {
    int winH[192 * 2];
    int winV[192 * 2];
    int winIn[192];
    int winOut[192];
};

bool checkBounds(int x, int y, int winNum) {
    bool winEnabled = (dispCnt & (1 << (13 + winNum))) != 0;
    if (!winEnabled) {
        return false;
    }

    int h = winH[winNum * 192 + y];
    int v = winV[winNum * 192 + y];

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

    int enabled = (winIn[y] >> (winNum * 8)) & 0xFF;
    color = vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
    return true;
}

void main() {
    int x = int(screenPos.x);
    int y = int(screenPos.y);

    if (!checkBounds(x, y, 0) && !checkBounds(x, y, 1)) {
        int enabled = winOut[y] & 0xFF;
        color = vec4(float(enabled) / 255.0, 0.0, 0.0, 0.0);
    }
}
