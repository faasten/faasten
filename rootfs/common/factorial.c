#include <stdlib.h>
int main(int argc, char * argv[]) {
    long int n = atol(argv[1]), i;
    unsigned long long fact = 1;

    for (i = 1; i <= n; ++i) {
            fact *= i;
    }

    return 0;
}
