<?php
require_once __DIR__ . '/models.php';

const GLOBAL_CONST = 100;

/**
 * Greet a user by name.
 */
function globalFunction(string $name): string {
    return "Hello, $name";
}

function add(int $x, int $y): int {
    return $x + $y;
}

$result = globalFunction("test");
