rule download_fasta:
    output:
        "results/reference/GRCh38.primary_assembly.genome.fa.gz",
    params:
        url=config["reference"]["fasta_url"],
    shell:
        "curl -L -o {output} {params.url}"


rule download_gtf:
    output:
        "results/reference/gencode.v{release}.annotation.gtf.gz",
    params:
        url=config["reference"]["gtf_url"],
    shell:
        "curl -L -o {output} {params.url}"
