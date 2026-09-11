def mandelbrot_count(width: int, height: int, max_iter: int) -> int:
    count = 0
    for y in range(height):
        for x in range(width):
            zi = 0.0
            zr = 0.0
            cr = (float(x) * 2.0) / float(width) - 1.5
            ci = (float(y) * 2.0) / float(height) - 1.0
            iter = 0
            while iter < max_iter:
                if zr * zr + zi * zi > 4.0:
                    iter = max_iter + 1
                else:
                    temp_zr = zr * zr - zi * zi + cr
                    zi = 2.0 * zr * zi + ci
                    zr = temp_zr
                    iter = iter + 1
            if iter == max_iter:
                count = count + 1
    return count

print(mandelbrot_count(200, 200, 50))
