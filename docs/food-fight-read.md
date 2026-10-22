# Food Fight 1.2 Windows startup crash

The supplied log (`com.tomatoflix.foodfight`) fails a read of 0x17fb5 bytes
with Windows error 998, then throws a C++ exception and aborts. The native
read was receiving a slice into lazily committed guest memory. Kernel-mode
I/O cannot invoke the user-mode exception handler that commits those pages.
Microsoft describes this constraint in [The Old New Thing](https://devblogs.microsoft.com/oldnewthing/20180124-00/?p=97875).

On Windows, POSIX file reads now use committed host storage, then copy only
the returned bytes into guest memory through user-mode code. Other platforms
keep the existing direct-read path. EOF and short reads preserve the unused
part of the destination. Allocation failure is returned as an I/O error.

A Windows regression test reads an actual file into untouched guest pages at
the size from the reported failure, verifies every byte, and checks that a
short read and EOF leave the remainder unchanged. In the real Food Fight
guest, the corrected run passed the failing read and reached the rendered
welcome/name-entry screen without the reported exception/abort. This verifies
the startup crash path, not a complete gameplay session.
