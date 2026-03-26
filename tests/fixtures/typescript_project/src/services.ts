import { User } from "./models";

export class UserService {
    private users: User[];

    constructor() {
        this.users = [];
    }

    addUser(user: User): void {
        this.users.push(user);
    }

    getUser(name: string): User | undefined {
        return this.users.find(u => u.name === name);
    }

    private validate(user: User): boolean {
        return user.name.length > 0 && user.age > 0;
    }
}

class InternalService {
    run(): void {}
}
