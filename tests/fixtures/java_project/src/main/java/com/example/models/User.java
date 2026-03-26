package com.example.models;

/** Represents a user in the system. */
public class User {
    private String name;
    private int age;

    /** Maximum allowed age. */
    public static final int MAX_AGE = 200;

    /** Create a new user with the given name and age. */
    public User(String name, int age) {
        this.name = name;
        this.age = age;
    }

    /** Get the user's name. */
    public String getName() {
        return name;
    }

    /** Get the user's age. */
    public int getAge() {
        return age;
    }

    /** Check if the user is an adult. */
    public boolean isAdult() {
        return age >= 18;
    }

    private void internalHelper() {
        // private method
    }

    protected String displayName() {
        return name.toUpperCase();
    }

    void packageMethod() {
        // package-private method
    }
}
