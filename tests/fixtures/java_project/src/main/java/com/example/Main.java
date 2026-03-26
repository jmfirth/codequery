package com.example;

import com.example.models.User;
import com.example.services.UserService;

/** Main application entry point. */
public class Main {
    public static void main(String[] args) {
        User user = new User("Alice", 30);
        System.out.println(user.getName());
    }
}
