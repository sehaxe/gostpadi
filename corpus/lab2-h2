#include <stdio.h>

//Задача 2. Вариант 5
//Вычислить число Пи по формуле Грегори, взяв 500 членов ряда:
//П/4 = 1 - 1/3 + 1/5 - 1/7 + ...

#define TERMS 500  //сколько членов ряда суммируем
#define STEP 2     //шаг знаменателя: нечётные числа 1, 3, 5, ...
#define FACTOR 4   //частичная сумма равна П/4, значит Пи = сумма * 4

int main(void) {
    double sum = 0;
    double denominator = 1;
    double sign = 1;

    for (int i = 0; i < TERMS; i++){
        sum = sum + sign / denominator;

        denominator = denominator + STEP;
        sign = sign * (-1);
    }

    double pi = sum * FACTOR;

    printf("Число Пи равно: %f\n", pi);

    return 0;
}
