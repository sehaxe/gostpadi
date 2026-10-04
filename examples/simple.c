#include <stdio.h>

int main(void) {
    int x;
    scanf("%d", &x);
    if (x > 0) {
        printf("positive");
    } else {
        printf("non-positive");
    }
    return 0;
}
