#include <stdio.h>
#include <math.h>

#define PI 3.1415926535

int main() {
    double x_deg, x_rad;
    int n, k;

    while (1) {
        printf("Vvedite ugol X (v gradusah): ");

        if (scanf("%lf", &x_deg) == 1) {
            x_rad = x_deg * PI / 180.0;
            if (fabs(sin(x_rad)) < 0.000001) {
                printf("Oshibka! Ctg ne sushestvuet. Poprobuyte eshe raz.\n\n");
                continue;
            }
            break;
        } else {
            printf("Nepravilniy vvod! Nuzhno vvesti chislo.\n");
            while (getchar() != '\n'); // Очистка буфера от букв
            printf("\n");
        }
    }
    double cos_sum = 1.0;
    double cos_term = 1.0;
    for (n = 1; n <= 7; n++) {
        cos_term = cos_term * (-x_rad * x_rad) / ((2 * n - 1) * (2 * n));
        cos_sum += cos_term;
    }
    double sin_sum = x_rad;
    double sin_term = x_rad;
    for (k = 1; k <= 7; k++) {
        sin_term = sin_term * (-x_rad * x_rad) / ((2 * k) * (2 * k + 1));
        sin_sum += sin_term;
    }

    double ctg_taylor = cos_sum / sin_sum;
    double ctg_math = 1.0 / tan(x_rad);

    printf("\n--- Rezultaty ---\n");
    printf("Cherez Taylora: %lf\n", ctg_taylor);
    printf("Cherez math.h:  %lf\n", ctg_math);

    return 0;
}
