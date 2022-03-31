# The key-value store
|uid(u64)|serialized objects(struct LabeledDirEntry/struct Directory/struct File)|
|--------|-----------------------------------------------------------------------|
|0       |the root directory's labeled direntry                                  |
|1       |the root directory                                                     |
|...     |...                                                                    |

# Directories
Directories are tables mapping names to their labeled direntry.
|name(String)|labeled direntry(struct LabeledDirEntry)|
|------------|----------------------------------------|
|...         |...                                     |

# Labeled direntries
Labeled direntries have three fields, label, entry\_type, and uid. They are pointers
(uid) to the actual data objects, directories or files, with metadata (entry\_type)
and security policy (label).

# Files
Files are arrays of bytes
