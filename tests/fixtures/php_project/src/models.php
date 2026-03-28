<?php
namespace App\Models;

class User {
    public const MAX_AGE = 150;

    public function __construct(private string $name) {}

    public function getName(): string {
        return $this->name;
    }

    protected function validate(): bool {
        return true;
    }

    private function internalCheck(): void {}
}

interface Greeter {
    public function greet(): string;
}

trait Loggable {
    public function log(string $message): void {
        echo $message;
    }
}
