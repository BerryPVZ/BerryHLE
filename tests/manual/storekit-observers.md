# StoreKit observer regression checks

Use a guest test app with two SKPaymentTransactionObserver instances (A and B).
These checks exercise local emulation only; no genuine App Store receipts are
created and server-side receipt validation is not supported.

1. Enable IN-APP in the Cheat Engine overlay. Add a payment before registering
   any observer. Register A: it should receive the unfinished purchased
   transaction. Register A again: no duplicate callback should occur.
2. Register B: it should receive the same unfinished transaction. Have A call
   finishTransaction: in subsequent callbacks; B must still be able to read the
   transaction and its payment safely from the delivered array.
3. Remove A, then submit another payment: only B should receive it. Removing A
   again or removing an unregistered observer must not disconnect B.
4. In A's callback, remove A and B. The current delivery snapshot should remain
   safe; subsequent purchases must not notify either removed observer.
5. Restore purchases with A and B registered: both should receive restored
   transactions and, if implemented, the restore-finished callback. An observer
   without that optional method must not crash, including with IN-APP disabled.
6. Disable IN-APP: canMakePayments remains false and addPayment: only delivers
   empty transaction arrays; no purchased transactions should be fabricated.

7. Submit an SKMutablePayment with quantity 5, username and requestData. Change
   all fields after addPayment:. The transaction's payment must retain the
   original values. Check copy returns SKPayment and mutableCopy returns
   SKMutablePayment, preserving all fields, including the sandbox flag.
8. Read transactionDate twice with a delay: both reads must return the same
   date. Finish the transaction inside a callback and read it from the callback
   array again; its date and payment must remain accessible.
9. Start a products request, then disable IN-APP before its timer fires: no
   fabricated product response should arrive. Re-enable and start again: the
   normal product response should arrive. Cancel before delivery: no response.
