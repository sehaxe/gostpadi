#include <stdio.h>

int main(void) {
    double radius, ch;
    /* scanf_s — вариант MSVC с лишним аргументом размера буфера.
       Раньше усечение в парсере роняло здесь &radius. */
    scanf_s("%lf%c", &radius, &ch, 1);
    if (radius > 0) {
        printf("P = %.2f\n", 6.28 * radius);
    }
    return 0;
}
