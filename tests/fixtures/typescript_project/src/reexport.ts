/**
 * Re-export module that aggregates exports from other modules.
 */

export { User, Role } from "./models";
export { greet } from "./index";

export type AppUser = {
    user: import("./models").User;
    role: import("./models").Role;
};
