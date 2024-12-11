import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolventLabs } from "../target/types/solvent_labs";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  approve,
} from "@solana/spl-token";
import { assert } from "chai";

describe("solvent_labs", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolventLabs as Program<SolventLabs>;

  // Test accounts
  const owner = anchor.web3.Keypair.generate();
  const company = anchor.web3.Keypair.generate();
  const pool = anchor.web3.Keypair.generate();
  const host = anchor.web3.Keypair.generate();
  const joiner = anchor.web3.Keypair.generate();
  let state = anchor.web3.Keypair.generate();

  // PDAs and other accounts
  let vault: anchor.web3.PublicKey;
  let duel: anchor.web3.PublicKey;
  let tokenMint: anchor.web3.PublicKey;
  let hostTokenAccount: anchor.web3.PublicKey;
  let joinerTokenAccount: anchor.web3.PublicKey;
  let vaultTokenAccount: anchor.web3.PublicKey;
  let companyTokenAccount: anchor.web3.PublicKey;
  let poolTokenAccount: anchor.web3.PublicKey;

  const COMPANY_PERCENTAGE = 5;
  const POOL_PERCENTAGE = 5;
  const WAGER_AMOUNT = new anchor.BN(1_000_000);

  before(async () => {
    // Airdrop SOL to owner
    let signature = await provider.connection.requestAirdrop(
      owner.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(signature);

    signature = await provider.connection.requestAirdrop(
      host.publicKey,
      1 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(signature);

    signature = await provider.connection.requestAirdrop(
      joiner.publicKey,
      1 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(signature);

    // Create token mint
    tokenMint = await createMint(
      provider.connection,
      owner,
      owner.publicKey,
      null,
      6
    );

    // Find PDA addresses

    [vault] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault")],
      program.programId
    );

    // Create token accounts
    hostTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      tokenMint,
      host.publicKey
    );

    joinerTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      tokenMint,
      joiner.publicKey
    );

    companyTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      tokenMint,
      company.publicKey
    );

    poolTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      tokenMint,
      pool.publicKey
    );

    vaultTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      tokenMint,
      vault,
      undefined,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
      true
    );

    // Mint tokens to host and joiner
    await mintTo(
      provider.connection,
      owner,
      tokenMint,
      hostTokenAccount,
      owner.publicKey,
      WAGER_AMOUNT.toNumber() * 2
    );

    await mintTo(
      provider.connection,
      owner,
      tokenMint,
      joinerTokenAccount,
      owner.publicKey,
      WAGER_AMOUNT.toNumber() * 2
    );

    // Add token approvals for vault using spl-token approve
    await approve(
      provider.connection,
      host, // payer
      hostTokenAccount, // token account
      vault, // delegate
      host.publicKey, // owner
      WAGER_AMOUNT.toNumber()
    );

    await approve(
      provider.connection,
      joiner, // payer
      joinerTokenAccount, // token account
      vault, // delegate
      joiner.publicKey, // owner
      WAGER_AMOUNT.toNumber()
    );
  });

  it("Initializes the program", async () => {
    await program.methods
      .initialize(COMPANY_PERCENTAGE, POOL_PERCENTAGE)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
        company: company.publicKey,
        pool: pool.publicKey,
        tokenMint,
        vault,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([state, owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(stateAccount.owner.toBase58(), owner.publicKey.toBase58());
    assert.equal(stateAccount.companyPercentage, COMPANY_PERCENTAGE);
    assert.equal(stateAccount.poolPercentage, POOL_PERCENTAGE);
  });

  it("Creates a duel", async () => {
    [duel] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("duel"), new anchor.BN(0).toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    await program.methods
      .createDuel(WAGER_AMOUNT)
      .accounts({
        state: state.publicKey,
        duel,
        owner: owner.publicKey,
        host: host.publicKey,
        hostTokenAccount,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([owner])
      .rpc();

    const duelAccount = await program.account.duel.fetch(duel);
    assert.equal(duelAccount.host.toBase58(), host.publicKey.toBase58());
    assert.equal(duelAccount.wagerAmount.toString(), WAGER_AMOUNT.toString());
  });

  it("Joins a duel", async () => {
    await program.methods
      .joinDuel()
      .accounts({
        state: state.publicKey,
        duel,
        joiner: joiner.publicKey,
        joinerTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();

    const duelAccount = await program.account.duel.fetch(duel);
    assert.equal(duelAccount.joiner.toBase58(), joiner.publicKey.toBase58());
  });

  it("Starts a duel", async () => {
    await program.methods
      .startDuel()
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
        duel,
        host: host.publicKey,
        joiner: joiner.publicKey,
        tokenMint,
        hostTokenAccount,
        joinerTokenAccount,
        vaultTokenAccount,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([owner])
      .rpc();

    const duelAccount = await program.account.duel.fetch(duel);
    assert.equal(duelAccount.status.active !== undefined, true);
  });

  it("Distributes rewards", async () => {
    await program.methods
      .distributeRewards(host.publicKey)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
        duel,
        tokenMint,
        vaultTokenAccount,
        winnerTokenAccount: hostTokenAccount,
        companyTokenAccount,
        poolTokenAccount,
        winner: host.publicKey,
        company: company.publicKey,
        pool: pool.publicKey,
        vault,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([owner])
      .rpc();

    const duelAccount = await program.account.duel.fetch(duel);
    assert.equal(duelAccount.status.completed !== undefined, true);
    assert.equal(duelAccount.winner.toBase58(), host.publicKey.toBase58());
  });

  it("Updates company allocation", async () => {
    const newCompanyPercentage = 10;

    await program.methods
      .updateCompanyAllocation(newCompanyPercentage)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
      })
      .signers([owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(stateAccount.companyPercentage, newCompanyPercentage);
  });

  it("Updates pool allocation", async () => {
    const newPoolPercentage = 10;

    await program.methods
      .updatePoolAllocation(newPoolPercentage)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
      })
      .signers([owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(stateAccount.poolPercentage, newPoolPercentage);
  });

  it("Updates company address", async () => {
    const newCompany = anchor.web3.Keypair.generate();

    await program.methods
      .updateCompanyAddress(newCompany.publicKey)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
      })
      .signers([owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(
      stateAccount.company.toBase58(),
      newCompany.publicKey.toBase58()
    );
  });

  it("Updates pool address", async () => {
    const newPool = anchor.web3.Keypair.generate();

    await program.methods
      .updatePoolAddress(newPool.publicKey)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
      })
      .signers([owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(stateAccount.pool.toBase58(), newPool.publicKey.toBase58());
  });

  it("Updates token mint", async () => {
    const newTokenMint = await createMint(
      provider.connection,
      owner,
      owner.publicKey,
      null,
      6
    );

    await program.methods
      .updateTokenMint(newTokenMint)
      .accounts({
        state: state.publicKey,
        owner: owner.publicKey,
        newTokenMint,
      })
      .signers([owner])
      .rpc();

    const stateAccount = await program.account.state.fetch(state.publicKey);
    assert.equal(stateAccount.tokenMint.toBase58(), newTokenMint.toBase58());
  });
});
