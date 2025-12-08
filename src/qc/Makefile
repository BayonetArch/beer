CC = gcc
CFLAGS = -Wall -Wextra
SOURCE = qc.c
TARGET = build/qc
HEADER = include/essen.h

all: $(TARGET)

$(TARGET): $(SOURCE) $(HEADER)
	$(CC) $(CFLAGS) $< -o $@

clean:
	rm -f $(TARGET)

.PHONY: run all clean

run: $(TARGET)
	./$(TARGET)
