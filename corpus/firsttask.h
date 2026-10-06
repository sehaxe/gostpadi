#pragma once

void firstTask()
{
  double a, sum;
  int n = 0;
  printf("zadanie 1\n");
  printf("vvedite a\n");
  // восставлено ||, потерянное при пересылке
  if (scanf_s("%lf", &a) != 1 || a <= 1 || a >= 3) printf("oshibka!!!\n");
  else {
    sum = 0.;
    while (sum <= a) {
      n++;
      sum += 1. / n;
    }
    printf("n = %d\n", n);
  }
}
