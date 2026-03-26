CC = gcc
CFLAGS = -Wall -Wextra -std=c11

all: main

main: main.o utils.o
	$(CC) $(CFLAGS) -o main main.o utils.o

main.o: main.c utils.h
	$(CC) $(CFLAGS) -c main.c

utils.o: utils.c utils.h
	$(CC) $(CFLAGS) -c utils.c

clean:
	rm -f *.o main
