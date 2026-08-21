#include <stdlib.h>
#include <stdint.h>
#include <stdio.h>

typedef struct Node {
    int key;
    int val;
    struct Node *left;
    struct Node *right;
} Node;

static Node *insert(Node *root, int key, int val) {
    if (!root) {
        Node *n = malloc(sizeof(Node));
        if (!n) return NULL;
        n->key = key;
        n->val = val;
        n->left = n->right = NULL;
        return n;
    }
    if (key < root->key) root->left = insert(root->left, key, val);
    else if (key > root->key) root->right = insert(root->right, key, val);
    else root->val = val;
    return root;
}

static Node *search(Node *root, int key) {
    Node *curr = root;
    while (curr) {
        if (key == curr->key) return curr;
        if (key < curr->key) curr = curr->left;
        else curr = curr->right;
    }
    return NULL;
}

static void free_tree(Node *root) {
    if (!root) return;
    free_tree(root->left);
    free_tree(root->right);
    free(root);
}

int main(int argc, char **argv) {
    int n = (argc > 1) ? atoi(argv[1]) : 10000;
    int lookups = (argc > 2) ? atoi(argv[2]) : 200000;
    if (n < 1 || lookups < 1) return 1;

    uint32_t state = 0x1234567u;
    Node *root = NULL;
    for (int i = 0; i < n; i++) {
        state = state * 1664525u + 1013904223u;
        root = insert(root, (int)(state % (uint32_t)(n * 2)), i);
    }

    long hits = 0;
    for (int i = 0; i < lookups; i++) {
        state = state * 1664525u + 1013904223u;
        int key = (i % 5 == 0) ? (int)(state % 1000) : (int)(state % (uint32_t)(n * 2));
        Node *found = search(root, key);
        if (found) hits += found->val;
    }

    free_tree(root);
    return (hits >= 0) ? 0 : 2;
}
