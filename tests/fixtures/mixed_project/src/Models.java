package com.example;

/** A generic model entity. */
public class Models {
    private String id;
    private String value;

    public Models(String id, String value) {
        this.id = id;
        this.value = value;
    }

    /** Get the model ID. */
    public String getId() {
        return id;
    }

    /** Get the model value. */
    public String getValue() {
        return value;
    }
}
