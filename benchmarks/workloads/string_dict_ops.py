def main() -> int:
    text = "hello"
    letters = ""
    for ch in text:
        letters = letters + ch
    mapping = {"alpha": 10, "beta": 20}
    total = 0
    for k in mapping:
        total = total + mapping[k]
    print(len(letters) + total)
    return 0

if __name__ == "__main__":
    main()
