#include <stdio.h>
#include <math.h>

//Задача 1. Вариант 5

int main(void) {
    float a, b;

    printf("Введите катеты прямоугольного треугольника (через пробел, enter не имеет значения)\n");

    if (scanf("%f %f", &a, &b) != 2 || a <= 0 || b <= 0){
        printf("Ошибка ввода!\n");
        return 1;
    }

    printf("Гипотенуза: %.2f\n", sqrt(a * a + b * b));

    printf("Площадь: %.2f\n", a * b / 2);

    return 0;
}