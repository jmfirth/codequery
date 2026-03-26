/// Greet someone by name.
export function greet(name: string): string {
    return `Hello, ${name}!`;
}

function helper(): void {
    // internal helper, not exported
}

export const MAX_RETRIES = 3;

export const add = (a: number, b: number): number => a + b;

const localConst = 42;
