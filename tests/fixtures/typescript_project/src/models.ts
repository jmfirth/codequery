export interface User {
    name: string;
    age: number;
}

export interface Serializable {
    serialize(): string;
}

export type UserId = string;

export type Result<T> = { ok: true; value: T } | { ok: false; error: string };

export enum Role {
    Admin,
    User,
    Guest,
}
