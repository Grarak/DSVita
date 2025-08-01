#version 300 es

in vec2 position;
out vec2 block;

void main() {
    float x = position.x;
    int xInt = int(x);
    if (xInt % 8 != 0) {
        x += 1.0;
    }
    int xBlock = xInt & 0xF8;
    int yBlock = int(position.y) & 0xF8;
    block.x = float(xBlock);
    block.y = float(yBlock);
    gl_Position = vec4(x / 256.0 * 2.0 - 1.0, 1.0 - position.y / 191.0 * 2.0, 0.0, 1.0);
}
