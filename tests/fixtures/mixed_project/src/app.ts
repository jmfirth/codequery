import { User } from './models';

export interface AppConfig {
    name: string;
    debug: boolean;
}

export class Application {
    private config: AppConfig;

    constructor(config: AppConfig) {
        this.config = config;
    }

    run(): void {
        console.log(`Running ${this.config.name}`);
    }
}

export function createApp(name: string): Application {
    return new Application({ name, debug: false });
}

export const VERSION = "1.0.0";
