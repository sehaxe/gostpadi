// второму include нужен secondTask.h — его не прислали
#include <stdio.h>

#include "firstTask.h"
#include "secondTask.h"

int run(int state);
int update(int state);

int main()
{
  int state = 1;
  run(state);

  return 0;
}

int run(int state)
{
  while (state != 0)
  {
    state = update(state);
  }
}

int update(int state)
{
  printf("menu\n0 - vihod\n1, 2, 3, 4 - zadania\n");
  if (scanf_s("%d", &state) != 1) {
    printf("Error! Vvedite chislo\n");
    while (getchar() != '\n');
    return 1;
  }
  switch (state)
  {
  case 0:
    printf("Bye!!!\n");
    return 0;
    break;
  case 1:
    firstTask();
    break;
  case 2:
    secondTask();
    break;
  default:
    printf("Error!\n");
  }
}
