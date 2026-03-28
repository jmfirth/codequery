const std = @import("std");
const utils = @import("utils.zig");

/// Greet a person by name.
pub fn greet(name: []const u8) []const u8 {
    _ = name;
    return "Hello!";
}

fn helper() void {}

pub const MAX_SIZE: usize = 1024;

const internal_limit: usize = 256;

const Point = struct {
    x: f64,
    y: f64,
};

pub const Color = enum {
    red,
    green,
    blue,
};

const Tagged = union(enum) {
    int: i32,
    float: f64,
};

test "basic greet" {
    const result = greet("world");
    _ = result;
}

test "helper works" {
    helper();
}

pub fn main() void {
    const stdout = std.io.getStdOut().writer();
    stdout.print("Hello, world!\n", .{}) catch {};
}
