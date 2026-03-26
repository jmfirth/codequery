package com.example.services;

import com.example.models.User;

/** Service interface for user operations. */
public interface UserService {
    /** Find a user by their ID. */
    User findById(int id);

    /** Save a user to the store. */
    void save(User user);

    /** Delete a user by their ID. */
    void delete(int id);
}
